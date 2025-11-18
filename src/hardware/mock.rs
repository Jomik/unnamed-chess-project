use crate::game_state::{Bitboard, PieceSensor};

pub struct MockPieceSensor {
    bitboard: Bitboard,
}

impl MockPieceSensor {
    pub fn new() -> Self {
        Self {
            bitboard: Bitboard::new(0),
        }
    }

    pub fn toggle(&mut self, square: crate::game_state::Square) {
        self.bitboard.toggle(square);
    }
}

impl PieceSensor for MockPieceSensor {
    fn read_positions(&mut self) -> Bitboard {
        self.bitboard
    }
}
