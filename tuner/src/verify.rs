use std::fs::File;
use std::io::{BufReader, ErrorKind, Read};
use std::path::Path;
use mythos::eval::eval::eval_score;
use mythos::eval::king_safety::king_safety;
use mythos::eval::S;
use mythos::eval::trace::initial_weights;
use crate::data::parse_data;
use crate::format;
use crate::format::{Record, SIZE};

pub struct Reader {
    rec: BufReader<File>,
    arena: BufReader<File>,
    coeffs: Vec<u16>,
    offset: u64,
    count: u64,
    total: u64
}

impl Reader {
    pub fn open(dir: &str) -> Self {
        let entries_file = File::open(Path::new(dir).join("entries.arena")).expect("Can't open arena file");
        let rec_file = File::open(Path::new(dir).join("entries.rec")).expect("Can't open rec file");

        let len = rec_file.metadata().expect("Can't stat rec file").len();
        assert_eq!(len % SIZE as u64, 0,
            "entries.rec is {len} bytes, not a multiple of {SIZE} — truncated write?");

        Self {
            rec: BufReader::with_capacity(1 << 20, rec_file),
            arena: BufReader::with_capacity(1 << 20, entries_file),
            coeffs: Vec::new(),
            offset: 0,
            count: 0,
            total: len / SIZE as u64
        }

    }

    pub fn total(&self) -> u64 {
        self.total
    }

    // Records handed out so far.
    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn next(&mut self) -> Option<(Record, &[u16])> {
        let mut buf = [0u8; SIZE];
        match self.rec.read_exact(&mut buf) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return None,
            Err(e) => panic!("rec read failed at record {}: {e}", self.count),
        }

        let record = Record::from_byte(&buf);

        assert_eq!(record.start, self.offset,
            "record {}: start chain broken, expected {} got {}", self.count, self.offset, record.start);

        self.coeffs.clear();
        for i in 0..record.len {
            let mut pair = [0u8; 2];
            match self.arena.read_exact(&mut pair) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::UnexpectedEof =>
                    panic!("record {}: arena truncated {i} coefficients into {}", self.count, record.len),
                Err(e) => panic!("arena read failed at record {}: {e}", self.count),
            }

            self.coeffs.push(u16::from_le_bytes(pair));
        }

        self.offset += record.len as u64;
        self.count  += 1;

        Some((record, &self.coeffs))
    }

    pub fn finish(mut self) -> u64 {
        let mut byte = [0u8; 1];

        if self.rec.read(&mut byte).expect("rec read failed") != 0 {
            panic!("entries.rec holds more records than the EPD holds positions (read {})", self.count);
        }

        if self.arena.read(&mut byte).expect("arena read failed") != 0 {
            panic!("entries.arena has trailing data after {} records", self.count);
        }

        self.count
    }
}

pub fn verify(epd: &str, dir: &str, limit: Option<usize>) {
    let weights = initial_weights();
    let mut reader = Reader::open(dir);

    // Copied out before the closure borrows the reader mutably.
    let records = reader.total();
    println!("verifying {records} records against {epd}");

    parse_data(epd, limit, |board, result| {
        let (_, coeffs) = match reader.next() {
            Some(entry) => entry,
            // Not corruption: stage 1 simply wrote fewer positions than stage 2 is
            // reading, which means the two ran with different --limit values.
            None => panic!("entries.rec holds only {records} records but the EPD has more \
                            — re-run prepare with the --limit verify is using"),
        };
        let mut total = S(0, 0);
        for coeff in coeffs {
            let (index, value) = format::unpack(*coeff);
            total += weights[index] * value as i32;
        }
        let real_total = eval_score(board) - king_safety(board);
        if real_total != total {
            println!("Trigger");
        }

    }

    );
    reader.finish();
    println!("Finish");

}

