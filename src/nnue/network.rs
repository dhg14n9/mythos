use crate::board::board::Board;
use crate::nnue::accumulator::{feature_index, king_context, needs_refresh, Accumulator, Delta, AccState, FinnyTable};
use crate::nnue::{HL, INPUT, OUTPUT_BUCKETS, QA, QB, SCALE, L1, L2, FT_SHIFT};
use crate::types::{Color, Piece, Square};

const L1_SCALE: f32 = (1 << FT_SHIFT) as f32 / (QA as f32 * QB as f32 * QA as f32);

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

const NNZ_TABLE: [[u16; 8]; 256] = {
    let mut table = [[0u16; 8]; 256];
    let mut mask = 0;
    while mask < 256 {
        let mut n = 0;
        let mut bit = 0;
        while bit < 8 {
            if mask & (1 << bit) != 0 {
                table[mask][n] = bit;
                n += 1;
            }
            bit += 1;
        }
        mask += 1;
    }
    table
};

const _: () = assert!(NNZ_TABLE[0b1010_0000][0] == 5 && NNZ_TABLE[0b1010_0000][1] == 7);
const _: () = assert!(NNZ_TABLE[0xFF][7] == 7);

#[repr(C)]
pub struct Network {
    feature_weights: [Accumulator; INPUT],
    feature_bias: Accumulator,
    l1_weights: [[[i8; L1 * 4]; HL / 4]; OUTPUT_BUCKETS],
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

    pub const fn transpose_l1(&mut self) {
        let mut bucket = 0;
        while bucket < OUTPUT_BUCKETS {
            let old = self.l1_weights[bucket];

            let mut o = 0;
            while o < L1 {
                let mut j = 0;
                while j < HL {
                    let p = o * HL + j;
                    let w = old[p / (L1 * 4)][p % (L1 * 4)];
                    self.l1_weights[bucket][j / 4][o * 4 + j % 4] = w;

                    j += 1;
                }
                o += 1;
            }
            bucket += 1;
        }
    }
}

pub fn load_net(path: &str) -> Box<Network> {
    let bytes = std::fs::read(path).expect("failed to load net file");
    assert_eq!(bytes.len(), size_of::<Network>());

    let mut net: Box<std::mem::MaybeUninit<Network>> = Box::new_uninit();

    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), net.as_mut_ptr().cast::<u8>(), size_of::<Network>());
        net.assume_init_mut().transpose_l1();
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
    let mut input = [0u8; HL];
    pairwise_int(us, &mut input[..HL / 2]);
    pairwise_int(them, &mut input[HL / 2..]);
    crate::nnue::stats::record_l1_input(&input);

    let h1 = l1_int(net, &input, bucket);
    let h2 = l2(net, &h1, bucket);
    let out = l3(net, &h2, bucket);

    (out * SCALE as f32) as i32
}

#[inline]
pub(crate) fn pairwise_int(acc: &Accumulator, out: &mut [u8]) {
    #[cfg(target_feature = "avx2")]
    {
        pairwise_int_avx2(acc, out)
    }
    #[cfg(not(target_feature = "avx2"))]
    {
        pairwise_int_scalar(acc, out)
    }
}

pub fn pairwise_int_scalar(acc: &Accumulator, out: &mut [u8]) {
    for i in 0..HL / 2 {
        let a = acc.get(i);
        let b = acc.get(i + HL / 2);

        let a = a.clamp(0, QA);
        let b = b.clamp(0, QA);

        out[i] = ((i32::from(a) * i32::from(b)) >> FT_SHIFT) as u8;
    }
}

// mulhi(a << 7, b) = (a * b) >> 9 without leaving i16: a << 7 <= 255 * 128 fits.
#[cfg(target_feature = "avx2")]
#[inline]
pub fn pairwise_int_avx2(acc: &Accumulator, out: &mut [u8]) {
    use std::arch::x86_64::*;

    const LANES: usize = 16;
    const _: () = assert!(HL / 2 % (2 * LANES) == 0, "HL / 2 must be a multiple of 32 for the AVX2 pairwise");
    const _: () = assert!(FT_SHIFT == 9, "the mulhi trick is exact only for a shift of 9");
    assert_eq!(out.len(), HL / 2);

    unsafe {
        let zero = _mm256_setzero_si256();
        let upper = _mm256_set1_epi16(QA);
        let values = acc.as_slice().as_ptr();

        let product = |i: usize| {
            let a = _mm256_load_si256(values.add(i).cast());
            let b = _mm256_load_si256(values.add(i + HL / 2).cast());
            let a = _mm256_min_epi16(_mm256_max_epi16(a, zero), upper);
            let b = _mm256_min_epi16(_mm256_max_epi16(b, zero), upper);
            _mm256_mulhi_epi16(_mm256_slli_epi16::<7>(a), b)
        };

        let mut i = 0;
        while i < HL / 2 {
            // packus interleaves 128-bit lanes
            let packed = _mm256_packus_epi16(product(i), product(i + LANES));
            let packed = _mm256_permute4x64_epi64::<0b11_01_10_00>(packed);
            _mm256_storeu_si256(out.as_mut_ptr().add(i).cast(), packed);
            i += 2 * LANES;
        }
    }
}

#[inline]
pub(crate) fn l1_int(net: &Network, input: &[u8; HL], bucket: usize) -> [f32; L1] {
    #[cfg(target_feature = "avx2")]
    {
        l1_int_avx2(net, input, bucket)
    }
    #[cfg(not(target_feature = "avx2"))]
    {
        l1_int_scalar(net, input, bucket)
    }
}

pub fn l1_int_scalar(net: &Network, input: &[u8; HL], bucket: usize) -> [f32; L1] {
    let mut output = [0f32; L1];

    for i in 0..L1 {
        let mut sum: i32 = 0;
        for j in 0..HL {
            sum += i32::from(input[j]) * i32::from(net.l1_weights[bucket][j / 4][i * 4 + j % 4]);
        }
        let mut sum = sum as f32 * L1_SCALE;
        sum += net.l1_bias[bucket][i];
        output[i] = screlu(sum);
    }

    output
}

#[inline]
pub(crate) fn find_nnz(input: &[u8; HL], nnz: &mut [u16; HL / 4]) -> usize {
    #[cfg(target_feature = "avx2")]
    {
        find_nnz_avx2(input, nnz)
    }
    #[cfg(not(target_feature = "avx2"))]
    {
        find_nnz_scalar(input, nnz)
    }
}

pub fn find_nnz_scalar(input: &[u8; HL], nnz: &mut [u16; HL / 4]) -> usize {
    let mut count = 0;
    for (i, chunk) in input.chunks_exact(4).enumerate() {
        let x = u32::from_ne_bytes(chunk.try_into().unwrap());
        if x != 0 {
            nnz[count] = i as u16;
            count += 1;
        }
    }
    count
}

#[cfg(target_feature = "avx2")]
#[inline]
pub fn find_nnz_avx2(input: &[u8; HL], nnz: &mut [u16; HL / 4]) -> usize {
    use std::arch::x86_64::*;

    const _: () = assert!(HL % 32 == 0);

    let mut count = 0;

    unsafe {
        let zero = _mm256_setzero_si256();
        let step = _mm_set1_epi16(8);
        let mut base = _mm_setzero_si128();

        for i in (0..HL).step_by(32) {
            let v = _mm256_loadu_si256(input.as_ptr().add(i).cast());
            let mask = _mm256_movemask_ps(_mm256_castsi256_ps(_mm256_cmpgt_epi32(v, zero))) as usize;

            let offsets = _mm_loadu_si128(NNZ_TABLE[mask].as_ptr().cast());
            _mm_storeu_si128(nnz.as_mut_ptr().add(count).cast(), _mm_add_epi16(base, offsets));

            count += mask.count_ones() as usize;
            base = _mm_add_epi16(base, step);
        }
    }

    count
}

// maddubs cannot saturate: inputs are <= 127, so a pair sums to at most 2 * 127 * 128.
#[cfg(target_feature = "avx2")]
#[inline]
pub fn l1_int_avx2(net: &Network, input: &[u8; HL], bucket: usize) -> [f32; L1] {
    use std::arch::x86_64::*;

    const _: () = assert!(L1 == 16, "two 8-lane accumulators cover L1");
    const UNROLL: usize = 4;

    let weights = &net.l1_weights[bucket];
    let mut nnz = [0u16; HL / 4];
    let count = find_nnz(input, &mut nnz);
    let mut output = [0f32; L1];

    unsafe {
        let chunks = input.as_ptr().cast::<i32>();
        let mut lo = [_mm256_setzero_si256(); UNROLL];
        let mut hi = [_mm256_setzero_si256(); UNROLL];

        let accumulate = |lo: &mut __m256i, hi: &mut __m256i, c: u16| {
            let c = c as usize;
            let x = _mm256_set1_epi32(chunks.add(c).read_unaligned());
            let w = weights[c].as_ptr();
            *lo = dpbusd(*lo, x, _mm256_loadu_si256(w.cast()));
            *hi = dpbusd(*hi, x, _mm256_loadu_si256(w.add(32).cast()));
        };

        // independent accumulators hide dpbusd latency; one pair would serialise the loop
        let mut groups = nnz[..count].chunks_exact(UNROLL);
        for group in &mut groups {
            for k in 0..UNROLL {
                accumulate(&mut lo[k], &mut hi[k], group[k]);
            }
        }
        for &c in groups.remainder() {
            accumulate(&mut lo[0], &mut hi[0], c);
        }

        for k in 1..UNROLL {
            lo[0] = _mm256_add_epi32(lo[0], lo[k]);
            hi[0] = _mm256_add_epi32(hi[0], hi[k]);
        }
        let (lo, hi) = (lo[0], hi[0]);

        let scale = _mm256_set1_ps(L1_SCALE);
        let bias = net.l1_bias[bucket].as_ptr();

        for (i, sums) in [(0, lo), (8, hi)] {
            // mul then add, not FMA: keeps the rounding identical to l1_int_scalar
            let sums = _mm256_mul_ps(_mm256_cvtepi32_ps(sums), scale);
            let sums = _mm256_add_ps(sums, _mm256_loadu_ps(bias.add(i)));
            _mm256_storeu_ps(output.as_mut_ptr().add(i), screlu_ps(sums));
        }
    }

    output
}

#[cfg(all(target_feature = "avx2", target_feature = "avxvnni"))]
#[inline]
unsafe fn dpbusd(acc: std::arch::x86_64::__m256i, x: std::arch::x86_64::__m256i, w: std::arch::x86_64::__m256i) -> std::arch::x86_64::__m256i {
    unsafe { std::arch::x86_64::_mm256_dpbusd_avx_epi32(acc, x, w) }
}

#[cfg(all(target_feature = "avx2", not(target_feature = "avxvnni")))]
#[inline]
unsafe fn dpbusd(acc: std::arch::x86_64::__m256i, x: std::arch::x86_64::__m256i, w: std::arch::x86_64::__m256i) -> std::arch::x86_64::__m256i {
    use std::arch::x86_64::*;

    unsafe { _mm256_add_epi32(acc, _mm256_madd_epi16(_mm256_maddubs_epi16(x, w), _mm256_set1_epi16(1))) }
}

#[inline]
fn l2(net: &Network, input: &[f32; L1], bucket: usize) -> [f32; L2] {
    #[cfg(all(target_feature = "avx2", target_feature = "fma"))]
    {
        l2_avx2(net, input, bucket)
    }
    #[cfg(not(all(target_feature = "avx2", target_feature = "fma")))]
    {
        l2_scalar(net, input, bucket)
    }
}

pub fn l2_scalar(net: &Network, input: &[f32; L1], bucket: usize) -> [f32; L2] {
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

// Eight row dot products at a time; the hadd tree reduces them to one vector of eight sums.
#[cfg(all(target_feature = "avx2", target_feature = "fma"))]
#[inline]
pub fn l2_avx2(net: &Network, input: &[f32; L1], bucket: usize) -> [f32; L2] {
    use std::arch::x86_64::*;

    const LANES: usize = 8;
    const _: () = assert!(L1 == 2 * LANES && L2 % LANES == 0);

    let weights = &net.l2_weights[bucket];
    let mut output = [0f32; L2];

    unsafe {
        let lo = _mm256_loadu_ps(input.as_ptr());
        let hi = _mm256_loadu_ps(input.as_ptr().add(LANES));

        for n in (0..L2).step_by(LANES) {
            let row = |k: usize| {
                let w = weights[n + k].as_ptr();
                _mm256_fmadd_ps(hi, _mm256_loadu_ps(w.add(LANES)), _mm256_mul_ps(lo, _mm256_loadu_ps(w)))
            };

            let s01 = _mm256_hadd_ps(row(0), row(1));
            let s23 = _mm256_hadd_ps(row(2), row(3));
            let s45 = _mm256_hadd_ps(row(4), row(5));
            let s67 = _mm256_hadd_ps(row(6), row(7));
            let s0123 = _mm256_hadd_ps(s01, s23);
            let s4567 = _mm256_hadd_ps(s45, s67);
            let sums = _mm256_add_ps(
                _mm256_permute2f128_ps::<0x20>(s0123, s4567),
                _mm256_permute2f128_ps::<0x31>(s0123, s4567),
            );

            let sums = _mm256_add_ps(sums, _mm256_loadu_ps(net.l2_bias[bucket].as_ptr().add(n)));
            _mm256_storeu_ps(output.as_mut_ptr().add(n), screlu_ps(sums));
        }
    }

    output
}

#[inline]
fn l3(net: &Network, input: &[f32; L2], bucket: usize) -> f32 {
    #[cfg(all(target_feature = "avx2", target_feature = "fma"))]
    {
        l3_avx2(net, input, bucket)
    }
    #[cfg(not(all(target_feature = "avx2", target_feature = "fma")))]
    {
        l3_scalar(net, input, bucket)
    }
}

#[cfg(all(target_feature = "avx2", target_feature = "fma"))]
#[inline]
pub fn l3_avx2(net: &Network, input: &[f32; L2], bucket: usize) -> f32 {
    use std::arch::x86_64::*;

    const LANES: usize = 8;
    const _: () = assert!(L2 % LANES == 0);

    let weights = &net.l3_weights[bucket];

    unsafe {
        let mut acc = _mm256_setzero_ps();
        for i in (0..L2).step_by(LANES) {
            acc = _mm256_fmadd_ps(_mm256_loadu_ps(input.as_ptr().add(i)), _mm256_loadu_ps(weights.as_ptr().add(i)), acc);
        }

        hsum_ps(acc) + net.l3_bias[bucket]
    }
}

#[cfg(target_feature = "avx2")]
#[inline]
unsafe fn screlu_ps(x: std::arch::x86_64::__m256) -> std::arch::x86_64::__m256 {
    use std::arch::x86_64::*;

    unsafe {
        let x = _mm256_min_ps(_mm256_max_ps(x, _mm256_setzero_ps()), _mm256_set1_ps(1.0));
        _mm256_mul_ps(x, x)
    }
}

#[cfg(target_feature = "avx2")]
#[inline]
unsafe fn hsum_ps(v: std::arch::x86_64::__m256) -> f32 {
    use std::arch::x86_64::*;

    unsafe {
        let s = _mm_add_ps(_mm256_castps256_ps128(v), _mm256_extractf128_ps::<1>(v));
        let s = _mm_add_ps(s, _mm_movehl_ps(s, s));
        let s = _mm_add_ss(s, _mm_movehdup_ps(s));
        _mm_cvtss_f32(s)
    }
}

pub fn l3_scalar(net: &Network, input: &[f32; L2], bucket: usize) -> f32 {
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
