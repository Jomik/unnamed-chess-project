use super::Bitboard;

/// A trait for reading piece positions from physical sensors.
///
/// Implementations abstract the hardware interface for detecting chess pieces,
/// whether through actual sensors (hall effect) or mock implementations for testing.
pub trait PieceSensor {
    /// Reads the current positions of all pieces on the board.
    ///
    /// Returns a [`Bitboard`] where each set bit represents a square with a piece present.
    /// Bit 0 corresponds to square a1, bit 63 to h8.
    fn read_positions(&mut self) -> Bitboard;
}
