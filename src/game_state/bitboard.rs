/// Represents a single square on the chess board (0-63).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Square(u8);

impl Square {
    /// Creates a new Square if the index is valid (0-63).
    pub fn new(idx: u8) -> Option<Self> {
        if idx < 64 {
            Some(Square(idx))
        } else {
            None
        }
    }

    /// Returns the internal index value (0-63).
    pub fn value(&self) -> u8 {
        self.0
    }
}

/// A bitboard representing the state of the chess board.
///
/// Each bit represents one square: bit 0 = a1, bit 63 = h8.
/// A set bit (1) indicates a piece is present on that square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bitboard(u64);

impl Bitboard {
    /// Creates a new bitboard with the given value.
    pub fn new(value: u64) -> Self {
        Bitboard(value)
    }

    /// Returns the underlying u64 value.
    pub fn value(&self) -> u64 {
        self.0
    }

    /// Toggles the bit at the given square.
    pub fn toggle(&mut self, square: Square) {
        self.0 ^= 1 << square.value();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_square_creation() {
        assert!(Square::new(0).is_some());
        assert!(Square::new(63).is_some());
        assert!(Square::new(64).is_none());
        assert!(Square::new(255).is_none());
    }

    #[test]
    fn test_square_value() {
        assert_eq!(Square::new(0).unwrap().value(), 0);
        assert_eq!(Square::new(42).unwrap().value(), 42);
        assert_eq!(Square::new(63).unwrap().value(), 63);
    }

    #[test]
    fn test_bitboard_new() {
        let bb = Bitboard::new(0);
        assert_eq!(bb.value(), 0);

        let bb = Bitboard::new(0xFFFFFFFFFFFFFFFF);
        assert_eq!(bb.value(), 0xFFFFFFFFFFFFFFFF);
    }

    #[test]
    fn test_bitboard_toggle() {
        let mut bb = Bitboard::new(0);
        let square = Square::new(0).unwrap();

        bb.toggle(square);
        assert_eq!(bb.value(), 1);

        bb.toggle(square);
        assert_eq!(bb.value(), 0);
    }

    #[test]
    fn test_bitboard_multiple_squares() {
        let mut bb = Bitboard::new(0);

        bb.toggle(Square::new(0).unwrap()); // a1
        bb.toggle(Square::new(7).unwrap()); // h1
        bb.toggle(Square::new(63).unwrap()); // h8

        assert_eq!(bb.value(), 0x8000000000000081);
        assert_eq!(bb.value().count_ones(), 3);
    }

    #[test]
    fn test_bitboard_toggle_idempotent() {
        let mut bb = Bitboard::new(0);
        let square = Square::new(27).unwrap();

        bb.toggle(square);
        bb.toggle(square);
        bb.toggle(square);

        assert_eq!(bb.value(), 1 << 27);
    }
}
