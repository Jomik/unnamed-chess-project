use std::fmt;
use std::str::FromStr;

/// Represents a single square on the chess board (0-63).
///
/// Internally stores index 0-63 where:
/// - 0 = a1, 7 = h1
/// - 56 = a8, 63 = h8
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

    /// Returns the file ('a'-'h') of this square.
    pub fn file(&self) -> char {
        (b'a' + (self.0 % 8)) as char
    }

    /// Returns the rank (1-8) of this square.
    pub fn rank(&self) -> u8 {
        (self.0 / 8) + 1
    }
}

/// Parse algebraic notation like "e4" into a Square.
///
/// # Examples
/// ```
/// # use unnamed_chess_project::game_state::Square;
/// let square: Square = "e4".parse().unwrap();
/// assert_eq!(square.value(), 28);
/// ```
impl FromStr for Square {
    type Err = SquareParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 2 {
            return Err(SquareParseError::WrongLength);
        }

        let file = s.chars().next().unwrap().to_ascii_lowercase();
        let rank = s
            .chars()
            .nth(1)
            .unwrap()
            .to_digit(10)
            .ok_or(SquareParseError::BadRank)?;

        if !('a'..='h').contains(&file) {
            return Err(SquareParseError::BadFile);
        }
        if !(1..=8).contains(&rank) {
            return Err(SquareParseError::BadRank);
        }

        let idx = ((rank as u8 - 1) * 8) + (file as u8 - b'a');
        Ok(Square(idx))
    }
}

/// Display square in algebraic notation (e.g., "e4").
impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.file(), self.rank())
    }
}

/// Error type for parsing square notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SquareParseError {
    /// Square notation must be exactly 2 characters
    WrongLength,
    /// File must be a letter from a-h
    BadFile,
    /// Rank must be a digit from 1-8
    BadRank,
}

impl fmt::Display for SquareParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SquareParseError::WrongLength => write!(f, "square must be 2 characters (e.g., 'e4')"),
            SquareParseError::BadFile => write!(f, "file must be a-h"),
            SquareParseError::BadRank => write!(f, "rank must be 1-8"),
        }
    }
}

impl std::error::Error for SquareParseError {}

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
    fn test_square_file_rank() {
        let a1 = Square::new(0).unwrap();
        assert_eq!(a1.file(), 'a');
        assert_eq!(a1.rank(), 1);

        let h8 = Square::new(63).unwrap();
        assert_eq!(h8.file(), 'h');
        assert_eq!(h8.rank(), 8);

        let e4 = Square::new(28).unwrap();
        assert_eq!(e4.file(), 'e');
        assert_eq!(e4.rank(), 4);
    }

    #[test]
    fn test_square_from_str() {
        assert_eq!("a1".parse::<Square>().unwrap().value(), 0);
        assert_eq!("h1".parse::<Square>().unwrap().value(), 7);
        assert_eq!("a8".parse::<Square>().unwrap().value(), 56);
        assert_eq!("h8".parse::<Square>().unwrap().value(), 63);
        assert_eq!("e4".parse::<Square>().unwrap().value(), 28);
    }

    #[test]
    fn test_square_from_str_case_insensitive() {
        assert_eq!("E4".parse::<Square>(), "e4".parse::<Square>());
        assert_eq!("A1".parse::<Square>(), "a1".parse::<Square>());
    }

    #[test]
    fn test_square_from_str_invalid() {
        assert!("".parse::<Square>().is_err());
        assert!("a".parse::<Square>().is_err());
        assert!("a9".parse::<Square>().is_err());
        assert!("i1".parse::<Square>().is_err());
        assert!("a0".parse::<Square>().is_err());
        assert!("abc".parse::<Square>().is_err());
    }

    #[test]
    fn test_square_display() {
        assert_eq!(Square::new(0).unwrap().to_string(), "a1");
        assert_eq!(Square::new(7).unwrap().to_string(), "h1");
        assert_eq!(Square::new(28).unwrap().to_string(), "e4");
        assert_eq!(Square::new(63).unwrap().to_string(), "h8");
    }

    #[test]
    fn test_square_roundtrip() {
        for idx in 0..64 {
            let square = Square::new(idx).unwrap();
            let str_repr = square.to_string();
            let parsed: Square = str_repr.parse().unwrap();
            assert_eq!(square, parsed);
        }
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
}
