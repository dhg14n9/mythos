use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use mythos::board::board::Board;
use mythos::eval::S;
use mythos::eval::trace::Trace;

use crate::data;
use crate::format::{Record, pack};

// Stage 1: EPD -> entries.rec + entries.arena. One position in, one record out,
// nothing accumulates — so the dataset size is bounded by disk, not by RAM.
pub struct Writer {
    rec:   BufWriter<File>,
    arena: BufWriter<File>,
    start: u64, // running index into the arena, in u16 units
    count: u64,
}

impl Writer {
    // Both files are created once, here. Creating them inside push would truncate
    // on every position and leave only the last one.
    pub fn create(dir: &str) -> Self {
        let open = |name: &str| {
            let path = Path::new(dir).join(name);
            let file = File::create(&path)
                .unwrap_or_else(|e| panic!("cannot create {}: {e}", path.display()));
            BufWriter::with_capacity(1 << 20, file)
        };

        Self { rec: open("entries.rec"), arena: open("entries.arena"), start: 0, count: 0 }
    }

    pub fn push(&mut self, board: &Board, result: f32) {
        let mut trace = Trace::new();
        trace.trace(board);

        // Coefficients first: len is not known until they have been counted, which
        // is only possible because the arena is a separate file.
        let mut len: u16 = 0;
        for (i, &value) in trace.coeffs().iter().enumerate() {
            if value != 0 {
                self.arena.write_all(&pack(i, value).to_le_bytes()).expect("arena write failed");
                len += 1;
            }
        }

        // Promotions can push the phase past 24; an unclamped eg weight would go
        // negative and flip the sign of the endgame half.
        let phase = trace.phase().clamp(0, Board::GAME_PHASE_MAX);

        let record = Record {
            start:  self.start,
            len,
            phase:  phase as u8,
            result: encode_result(result),
            frozen: taper(trace.frozen(), phase),
        };

        self.rec.write_all(&record.to_byte()).expect("rec write failed");

        self.start += len as u64;
        self.count += 1;
    }

    // Takes self by value, so the only way to get the count out is to flush first.
    // BufWriter flushes on drop but discards the error, which would turn a full
    // disk into a silently truncated file.
    pub fn finish(mut self) -> u64 {
        self.rec.flush().expect("rec flush failed");
        self.arena.flush().expect("arena flush failed");
        self.count
    }
}

// 0 / 1 / 2 = loss / draw / win, so stage 2 decodes with `result as f32 / 2.0`.
// Equality on f32 is exact here: these are the three literals parse_data produces.
fn encode_result(result: f32) -> u8 {
    if result == 0.0 { 0 }
    else if result == 0.5 { 1 }
    else if result == 1.0 { 2 }
    else { panic!("unexpected result {result}") }
}

// The eval tapers with truncating integer division; the tuner works in floats
// throughout, so frozen is tapered the same way as the rest of E. Expects a
// phase already clamped to 0..=24.
fn taper(score: S, phase: i32) -> f32 {
    debug_assert!((0..=Board::GAME_PHASE_MAX).contains(&phase));

    let eg_phase = Board::GAME_PHASE_MAX - phase;
    (score.0 * phase + score.1 * eg_phase) as f32 / Board::GAME_PHASE_MAX as f32
}

pub fn prepare(path: &str, dir: &str, limit: Option<usize>) {
    let mut writer = Writer::create(dir);

    data::parse_data(path, limit, |board, result| writer.push(board, result));

    println!("wrote {} entries", writer.finish());
}