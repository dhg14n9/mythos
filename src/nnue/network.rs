use crate::board::board::Board;
use crate::nnue::accumulator::{feature_index, king_context, needs_refresh, Accumulator, Delta, AccState, FinnyTable};
use crate::nnue::{HL, INPUT, OUTPUT_BUCKETS, QA, QB, SCALE, L1, L2};
use crate::types::{Color, Piece, Square};

const NET_BYTES: usize = {
    let raw = size_of::<[Accumulator; INPUT]>()  // feature_weights
        + size_of::<Accumulator>()                       // feature_bias
        + size_of::<[[[i8; HL]; L1]; OUTPUT_BUCKETS]>()  // l1_weights
        + size_of::<[[f32; L1]; OUTPUT_BUCKETS]>()       // l1_bias
        + size_of::<[[[f32; L1]; L2]; OUTPUT_BUCKETS]>() // l2_weights
        + size_of::<[[f32; L2]; OUTPUT_BUCKETS]>()       // l2_bias
        + size_of::<[[f32; L2]; OUTPUT_BUCKETS]>()       // l3_weights
        + size_of::<[f32; OUTPUT_BUCKETS]>();            // l3_bias
    let align = align_of::<Network>();
    (raw + align - 1) / align * align
};

const _: () = assert!(size_of::<Network>() == NET_BYTES);
const _: () = assert!(size_of::<Network>() == 15_880_768);

#[repr(C)]
pub struct Network {
    feature_weights: [Accumulator; INPUT],
    feature_bias: Accumulator,
    l1_weights: [[[i8; HL]; L1]; OUTPUT_BUCKETS],
    l1_bias: [[f32; L1]; OUTPUT_BUCKETS],
    l2_weights: [[[f32; L1]; L2]; OUTPUT_BUCKETS],
    l2_bias: [[f32; L2]; OUTPUT_BUCKETS],
    l3_weights: [[f32; L2]; OUTPUT_BUCKETS],
    l3_bias: [f32; OUTPUT_BUCKETS]
}

impl Network {
    pub fn feature_bias(&self) -> Accumulator {
        self.feature_bias
    }

    pub fn feature_weights(&self) -> &[Accumulator; INPUT] {
        &self.feature_weights
    }
}

pub fn load_net(path: &str) -> Box<Network> {
    let bytes = std::fs::read(path).expect("failed to load net file");
    assert_eq!(bytes.len(), size_of::<Network>());

    let mut net: Box<std::mem::MaybeUninit<Network>> = Box::new_uninit();

    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), net.as_mut_ptr().cast::<u8>(), size_of::<Network>());
        net.assume_init()
    }
}

pub fn refresh(net: &Network, board: &Board, perspective: Color) -> Accumulator {
    let mut result = net.feature_bias;
    let occ = board.occ();
    let (mirror, bucket) = king_context(board, perspective);

    for square in occ {
        let piece = board.piece_at(square);
        let index = feature_index(perspective, piece, square, mirror, bucket);
        result += net.feature_weights[index]
    }

    result
}

pub fn evaluate(net: &Network, us: &Accumulator, them: &Accumulator, bucket: usize) -> i32 {
    let mut input = [0f32; HL];
    pairwise(us, &mut input[..HL / 2]);
    pairwise(them, &mut input[HL / 2..]);

    let h1 = l1(net, &input, bucket);
    let h2 = l2(net, &h1, bucket);
    let out = l3(net, &h2, bucket);

    (out * SCALE as f32) as i32
}

pub(crate) fn pairwise(acc: &Accumulator, out: &mut [f32]) {
    for i in 0..HL / 2 {
        let a = acc.get(i);
        let b = acc.get(i + HL / 2);

        let a = a.clamp(0, QA);
        let b = b.clamp(0, QA);

        let a = (a as f32) / (QA as f32);
        let b = (b as f32) / (QA as f32);

        out[i] = a * b;
    }
}
pub(crate) fn l1(net: &Network, input: &[f32; HL], bucket: usize) -> [f32; L1] {
    let mut output = [0f32; L1];

    for i in 0..L1 {
        let mut sum: f32 = 0f32;
        for j in 0..HL {
            let weight = (net.l1_weights[bucket][i][j] as f32) / (QB as f32);
            sum += input[j] * weight;
        }
        sum += net.l1_bias[bucket][i];
        output[i] = screlu(sum);
    }

    output
}

fn l2(net: &Network, input: &[f32; L1], bucket: usize) -> [f32; L2] {
    let mut output = [0f32; L2];

    for i in 0..L2 {
        let mut sum: f32 = 0f32;
        for j in 0..L1 {
            sum += input[j] * net.l2_weights[bucket][i][j];
        }
        sum += net.l2_bias[bucket][i];

        output[i] = screlu(sum);
    }

    output
}

fn l3(net: &Network, input: &[f32; L2], bucket: usize) -> f32 {
    let mut sum: f32 = 0f32;
    for j in 0..L2 {
        sum += input[j] * net.l3_weights[bucket][j];
    }

    sum + net.l3_bias[bucket]
}

fn screlu(x: f32) -> f32 {
    x.clamp(0f32, 1f32).powi(2)
}

pub fn push(net: &Network, board: &Board, child: &mut AccState, delta: &Delta, finny_table: &mut FinnyTable) {
    for color in Color::ALL {
        let (mirror, bucket) = king_context(board, color);
        child.mirror[color] = mirror;
        child.bucket[color] = bucket;

        if needs_refresh(delta, color, mirror, bucket) {
            child.accs[color] = finny_table.refresh(net, board, color, mirror, bucket);
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
    // A deferred entry never changed weight block, so these describe its parent too.
    let mirror = child.mirror[color];
    let bucket = child.bucket[color];
    let delta = child.delta;

    let weights = |&(piece, square): &(Piece, Square)| {
        &net.feature_weights[feature_index(color, piece, square, mirror, bucket)]
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
