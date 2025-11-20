use shakmaty::{Bitboard, Square};

use crate::game_logic::PieceSensor;

/// Mock sensor for testing and development on non-ESP32 targets.
///
/// Maintains an in-memory bitboard that can be toggled via the terminal interface.
#[derive(Debug, Clone, Default)]
pub struct MockPieceSensor {
    bitboard: Bitboard,
}

impl MockPieceSensor {
    /// Creates a new mock sensor with an empty board.
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggles the piece presence at the given square.
    pub fn toggle(&mut self, square: Square) {
        self.bitboard.toggle(square);
    }
}

impl PieceSensor for MockPieceSensor {
    fn read_positions(&mut self) -> Bitboard {
        self.bitboard
    }
}
