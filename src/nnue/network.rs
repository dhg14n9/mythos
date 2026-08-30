use crate::board::board::Board;
use crate::nnue::accumulator::{feature_index, needs_refresh, should_mirror, Accumulator, Delta, AccState};
use crate::nnue::{HL, INPUT, QA, QB, SCALE};
use crate::types::{Color, Piece, Square};

const NET_BYTES: usize = {
    let raw = size_of::<[Accumulator; INPUT]>() // feature_weights
        + size_of::<Accumulator>()              // feature_bias
        + size_of::<[i16; 2 * HL]>()            // output_weights
        + size_of::<i16>();                     // output_bias
    let align = align_of::<Network>();
    (raw + align - 1) / align * align
};

const _: () = assert!(size_of::<Network>() == NET_BYTES);

#[repr(C)]
pub struct Network {
    feature_weights: [Accumulator; INPUT],
    feature_bias: Accumulator,
    output_weights: [i16; 2 * HL],
    output_bias: i16
}

impl Network {
    pub fn output_weights(&self) -> &[i16; 2 * HL] {
        &self.output_weights
    }
}

pub fn load_net(path: &str) -> Box<Network> {
    let bytes = std::fs::read(path).expect("failed to load net file");
    assert_eq!(bytes.len(), size_of::<Network>());
    Box::new(unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const Network) })

}

pub fn refresh(net: &Network, board: &Board, perspective: Color) -> Accumulator {
    let mut result = net.feature_bias;
    let occ = board.occ();
    let mirror = should_mirror(board, perspective);

    for square in occ {
        let piece = board.piece_at(square);
        let index = feature_index(perspective, piece, square, mirror);
        result += net.feature_weights[index]
    }

    result
}

pub fn evaluate(net: &Network, us: &Accumulator, them: &Accumulator) -> i32 {
    let mut sum = forward(net, us, them);

    sum /= QA as i32;
    sum += net.output_bias as i32;
    sum *= SCALE;
    sum /= (QA * QB) as i32;

    sum
}

#[inline]
pub fn forward(net: &Network, us: &Accumulator, them: &Accumulator) -> i32 {
    #[cfg(target_feature = "avx2")]
    {
        forward_avx2(net, us, them)
    }
    #[cfg(not(target_feature = "avx2"))]
    {
        forward_scalar(net, us, them)
    }
}

pub fn forward_scalar(net: &Network, us: &Accumulator, them: &Accumulator) -> i32 {
    let mut sum = 0;

    for i in 0..HL {
        sum += screlu(us.get(i)) * net.output_weights[i] as i32;
        sum += screlu(them.get(i)) * net.output_weights[HL + i] as i32;
    }

    sum
}

fn screlu(x: i16) -> i32 {
    let y = i32::from(x).clamp(0, i32::from(QA));
    y * y
}


#[cfg(target_feature = "avx2")]
#[inline]
fn forward_avx2(net: &Network, us: &Accumulator, them: &Accumulator) -> i32 {
    use std::arch::x86_64::*;

    const LANES: usize = 16;
    const _: () = assert!(HL % LANES == 0, "HL must be a multiple of 16 for the AVX2 path");

    unsafe {
        let zero = _mm256_setzero_si256();
        let upper = _mm256_set1_epi16(QA);
        let mut acc = _mm256_setzero_si256();

        for (side, offset) in [(us, 0usize), (them, HL)] {
            let values = side.as_slice().as_ptr();
            let weights = net.output_weights.as_ptr().add(offset);

            let mut i = 0;
            while i < HL {
                let x = _mm256_load_si256(values.add(i).cast());
                let x = _mm256_min_epi16(_mm256_max_epi16(x, zero), upper);

                let xw = _mm256_mullo_epi16(x, _mm256_loadu_si256(weights.add(i).cast()));

                acc = _mm256_add_epi32(acc, _mm256_madd_epi16(xw, x));

                i += LANES;
            }
        }
       let mut lanes = [0i32; 8];
        _mm256_storeu_si256(lanes.as_mut_ptr().cast(), acc);
        lanes.iter().sum()
    }
}

pub fn push(net: &Network, board: &Board, child: &mut AccState, delta: &Delta) {
    for color in Color::ALL {
        let mirror = should_mirror(board, color);
        child.mirror[color] = mirror;

        if needs_refresh(delta, color, mirror) {
            child.accs[color] = refresh(net, board, color);
            child.computed[color] = true;
        } else {
            child.computed[color] = false;
        }
    }
    child.delta = *delta;
}

pub fn materialize(net: &Network, stack: &mut [AccState], ply: usize, color: Color) {
    if stack[ply].computed[color] {
        return;
    }

    let mut oldest = ply;
    while oldest > 0 && !stack[oldest - 1].computed[color] {
        oldest -= 1;
    }
    debug_assert!(oldest > 0, "accumulator chain ran off the bottom of the stack");

    for i in oldest..=ply {
        let (head, tail) = stack.split_at_mut(i);
        apply(net, &head[i - 1], &mut tail[0], color);
    }
}

fn apply(net: &Network, parent: &AccState, child: &mut AccState, color: Color) {
    let mirror = child.mirror[color];
    let delta = child.delta;

    let weights = |&(piece, square): &(Piece, Square)| {
        &net.feature_weights[feature_index(color, piece, square, mirror)]
    };

    let parent = &parent.accs[color];
    match (delta.adds(), delta.subs()) {
        ([], []) => {
            child.accs[color] = *parent
        }
        ([a0], [s0]) => {
            child.accs[color].set_add_sub(parent, weights(a0), weights(s0))
        }
        ([a0], [s0, s1]) => {
            child.accs[color].set_add_sub2(parent, weights(a0), weights(s0), weights(s1))
        }
        ([a0, a1], [s0, s1]) => {
            child.accs[color].set_add2_sub2(parent, weights(a0), weights(a1), weights(s0), weights(s1))
        }
        (adds, subs) => {
            debug_assert!(false, "unhandled delta shape: {} adds, {} subs", adds.len(), subs.len());
            child.accs[color] = *parent;
            for add in adds {
                child.accs[color] += *weights(add)
            }
            for sub in subs {
                child.accs[color] -= *weights(sub)
            }
        }
    }

    child.computed[color] = true;
}
