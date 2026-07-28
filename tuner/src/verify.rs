use std::fs::File;
use std::io::{BufReader, ErrorKind, Read};
use std::path::Path;

use crate::format::{Record, SIZE};

pub struct Reader {
    rec: BufReader<File>,
    arena: BufReader<File>,
    coeffs: Vec<u16>,
    offset: u64,
    count: u64
}

impl Reader {
    pub fn open(dir: &str) -> Self {
        let entries_file = File::open(Path::new(dir).join("entries.arena")).expect("Can't open arena file");
        let rec_file = File::open(Path::new(dir).join("entries.rec")).expect("Can't open rec file");

        Self {
            rec: BufReader::with_capacity(1 << 20, rec_file),
            arena: BufReader::with_capacity(1 << 20, entries_file),
            coeffs: Vec::new(),
            offset: 0,
            count: 0
        }

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

