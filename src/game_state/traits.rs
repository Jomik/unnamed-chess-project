use super::Bitboard;

pub const NUM_SQUARES: usize = 64;

/// Read piece positions from physical sensors
pub trait PieceSensor {
    fn read_positions(&mut self) -> Bitboard;
}
