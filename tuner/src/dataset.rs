use std::fs::File;
use std::path::Path;
use memmap2::Mmap;
use mythos::eval::eval::eval_score;
use mythos::eval::king_safety::king_safety;
use mythos::eval::S;
use mythos::eval::trace::initial_weights;
use crate::data::parse_data;
use crate::format;
use crate::format::{Record, SIZE};

pub struct Dataset {
    rec: Mmap,
    arena: Mmap
}

impl Dataset {
    pub fn open(dir: &str) -> Self {
        let map = |name: &str| {
            let path = Path::new(dir).join(name);
            let file = File::open(&path)
                .unwrap_or_else(|e| panic!("cannot open {}: {e}", path.display()));

            let len = file.metadata().expect("cannot stat file").len();
            assert!(len > 0, "{} is empty — run prepare first", path.display());

            unsafe { Mmap::map(&file) }
                .unwrap_or_else(|e| panic!("cannot map {}: {e}", path.display()))
        };

        let dataset = Self { rec: map("entries.rec"), arena: map("entries.arena") };

        assert_eq!(dataset.rec.len() % SIZE, 0,
            "entries.rec is {} bytes, not a multiple of {SIZE} — truncated write?", dataset.rec.len());

        dataset.validate();
        dataset
    }

    pub fn records(&self) -> &[Record] {
        bytemuck::cast_slice(&self.rec)
    }

    pub fn arena(&self) -> &[u16] {
        bytemuck::cast_slice(&self.arena)
    }

    pub fn len(&self) -> usize {
        self.records().len()
    }

    pub fn entry(&self, i: usize) -> (&Record, &[u16]) {
        let record = &self.records()[i];
        let start = record.start as usize;

        (record, &self.arena()[start..start + record.len as usize])
    }

    fn validate(&self) {
        let arena = self.arena().len();
        let mut offset = 0usize;

        for (i, record) in self.records().iter().enumerate() {
            assert_eq!(record.start as usize, offset,
                "record {i}: start chain broken, expected {offset} got {}", record.start);

            offset += record.len as usize;

            assert!(offset <= arena,
                "record {i}: coefficients run {} past the end of the arena", offset - arena);
        }

        assert_eq!(offset, arena,
            "entries.arena has {} trailing u16 after {} records", arena - offset, self.len());
    }

    pub fn summary(&self) {
        let mut labels = [0u64; 3];
        for record in self.records() {
            labels[record.result as usize] += 1;
        }

        let n = self.len() as f64;
        let pct = |c: u64| 100.0 * c as f64 / n;

        println!("{} records, {} coefficients, mean {:.4} per entry",
            self.len(), self.arena().len(), self.arena().len() as f64 / n);
        println!("labels: {:.1}% 1-0 / {:.1}% draw / {:.1}% 0-1",
            pct(labels[2]), pct(labels[1]), pct(labels[0]));
    }
}

pub fn verify(epd: &str, dir: &str, limit: Option<usize>) {
    let weights = initial_weights();
    let dataset = Dataset::open(dir);
    let total = dataset.len();

    dataset.summary();
    println!("verifying {total} records against {epd}");

    let mut count = 0usize;
    let mut mismatches = 0u64;
    let mut first: Option<usize> = None;

    parse_data(epd, limit, |board, _result| {
        assert!(count < total,
            "entries.rec holds only {total} records but the EPD has more \
             — re-run prepare with the --limit verify is using");

        let (_, coeffs) = dataset.entry(count);

        let mut sum = S(0, 0);
        for coeff in coeffs {
            let (index, value) = format::unpack(*coeff);
            sum += weights[index] * value as i32;
        }

        let real_total = eval_score(board) - king_safety(board);
        if real_total != sum {
            mismatches += 1;
            first.get_or_insert(count);
        }

        count += 1;
    });

    assert_eq!(count, total,
        "read {count} positions from the EPD but entries.rec holds {total} records \
         — re-run prepare with the --limit verify is using");

    if mismatches > 0 {
        panic!("{mismatches} of {count} records disagree with the eval, first at record {}",
            first.unwrap());
    }

    println!("ok: {count} records match Σ c_i·w_i == eval_score − king_safety");
}