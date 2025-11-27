use shakmaty::{Bitboard, Chess, Position, Square};

/// Mock sensor for terminal visualization and testing.
///
/// Maintains an in-memory bitboard that can be toggled via the terminal interface.
#[derive(Debug, Clone, Default)]
pub struct MockPieceSensor {
    bitboard: Bitboard,
}

impl MockPieceSensor {
    /// Creates a new mock sensor with the starting chess position, minus the white king.
    #[inline]
    pub fn new() -> Self {
        let mut bitboard = Chess::default().board().occupied();
        // Remove E1 (white king square)
        bitboard.toggle(Square::E1);
        Self { bitboard }
    }

    #[inline]
    pub fn read_positions(&mut self) -> Bitboard {
        self.bitboard
    }

    /// Toggles the piece presence at the given square.
    #[inline]
    pub fn toggle(&mut self, square: Square) {
        self.bitboard.toggle(square);
    }

    /// Loads a complete bitboard position (for FEN loading).
    #[inline]
    pub fn load_bitboard(&mut self, bitboard: Bitboard) {
        self.bitboard = bitboard;
    }
}
