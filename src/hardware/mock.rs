use shakmaty::{Bitboard, Chess, Position, Square};

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
        let mut bitboard = Chess::default().board().occupied();
        // Remove E1 (white king square)
        bitboard.toggle(Square::E1);
        Self { bitboard }
    }

    pub fn read_positions(&mut self) -> Bitboard {
        self.bitboard
    }

    /// Toggles the piece presence at the given square.
    pub fn toggle(&mut self, square: Square) {
        self.bitboard.toggle(square);
    }
}
