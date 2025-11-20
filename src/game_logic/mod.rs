use shakmaty::Bitboard;

/// Trait for reading piece positions from physical sensors
pub trait PieceSensor {
    /// Read current piece positions as a bitboard
    fn read_positions(&mut self) -> Bitboard;
}
