
// a tracer's value index pair can be packed into a u16
// tracer's NUM_PARAMS = 462 < 512 so it will fit into 9 bits, I'll round that upto 10 bits
// if in the future I want to add more params. value will probably NOT exceed ±32 so 6 bits will be
// more than enough, value in [-32, 31] + 32 -> [0, 63] so it can be easily stored

use bytemuck::{Pod, Zeroable};

pub fn pack(index: usize, value: i16) -> u16 {
    debug_assert!((index < 1024) && ((-32..32).contains(&value)));

    index as u16 | (((value + 32) as u16) << 10)
}

pub fn unpack(packed: u16) -> (usize, i16) {
    ((packed & 1023) as usize, ((packed >> 10) as i16) - 32)
}

// Derived, not written down: the mapped reader casts bytes straight to Record, so a
// hand-typed 16 that ever disagreed with the real layout would pass the length check
// and then fail inside the cast. repr(C) plus Pod guarantee this is 16 with no padding.
pub const SIZE: usize = size_of::<Record>();

// pack Record into bytes
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct Record {
    pub start: u64,
    pub len: u16,
    pub phase: u8,
    pub result: u8, // 0 for w lose, 1 for draw, 2 for w win
    pub frozen: f32
}

impl Record {
    pub fn to_byte(&self) -> [u8; SIZE] {
        let mut result = [0; SIZE];
        result[0..8].copy_from_slice(&self.start.to_le_bytes());
        result[8..10].copy_from_slice(&self.len.to_le_bytes());
        result[10] = self.phase;
        result[11] = self.result;
        result[12..16].copy_from_slice(&self.frozen.to_le_bytes());

        result
    }

    pub fn from_byte(byte: &[u8; SIZE]) -> Self {
        Self {
            start: u64::from_le_bytes(byte[0..8].try_into().unwrap()),
            len: u16::from_le_bytes(byte[8..10].try_into().unwrap()),
            phase: byte[10],
            result: byte[11],
            frozen: f32::from_le_bytes(byte[12..16].try_into().unwrap())
        }
    }

    pub fn get_result(&self) -> f64 {
        self.result as f64 / 2.0
    }

}
