pub mod traits;

pub use traits::{PieceSensor, NUM_SQUARES};

/// Chess square (0-63: a1=0, h1=7, a8=56, h8=63)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Square(u8);

impl Square {
    pub const fn new(value: u8) -> Option<Self> {
        if value < NUM_SQUARES as u8 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub const fn value(self) -> u8 {
        self.0
    }
}

/// 64-bit bitboard (bit N = square N has a piece)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bitboard(u64);

impl Bitboard {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    pub fn toggle(&mut self, square: Square) {
        self.0 ^= 1u64 << square.value();
    }
}
