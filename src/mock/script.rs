use std::collections::VecDeque;

use shakmaty::{ByColor, Bitboard, Chess, Color, Position, Square};
use thiserror::Error;

/// Error when parsing or executing a board script.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ParseError {
    /// A square token could not be parsed.
    #[error("invalid square notation: '{0}'")]
    InvalidSquare(String),
    /// A piece was placed on an empty square without an explicit color prefix.
    #[error("{0}: missing color for piece placement")]
    MissingColor(String),
    /// A square is occupied by pieces of both colors, which is physically impossible.
    #[error("square(s) occupied by both colors: {0}")]
    OverlappingSquares(String),
}

/// A scriptable mock sensor that processes BoardScript format.
///
/// Maintains per-color bitboard state and executes script batches on demand.
/// New script can be appended at any time for interactive use.
#[derive(Debug, Clone)]
pub struct ScriptedSensor {
    positions: ByColor<Bitboard>,
    pending_batches: VecDeque<Vec<BatchEntry>>,
}

impl Default for ScriptedSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptedSensor {
    /// Create with starting chess position.
    pub fn new() -> Self {
        let chess = Chess::default();
        let board = chess.board();
        Self::from_bitboards(board.by_color(Color::White), board.by_color(Color::Black))
            .expect("starting position has no overlapping squares")
    }

    /// Create from separate white and black bitboards.
    ///
    /// Returns [`ParseError::OverlappingSquares`] if any square appears in both bitboards.
    pub fn from_bitboards(white: Bitboard, black: Bitboard) -> Result<Self, ParseError> {
        check_overlap(white, black)?;
        Ok(Self {
            positions: ByColor { white, black },
            pending_batches: VecDeque::new(),
        })
    }

    /// Per-color piece positions.
    #[inline]
    pub fn read_positions(&self) -> ByColor<Bitboard> {
        self.positions
    }

    /// Load separate white and black bitboards directly (e.g. when loading a FEN position).
    ///
    /// Returns [`ParseError::OverlappingSquares`] if any square appears in both bitboards.
    pub fn load_bitboards(&mut self, white: Bitboard, black: Bitboard) -> Result<(), ParseError> {
        check_overlap(white, black)?;
        self.positions = ByColor { white, black };
        self.pending_batches.clear();
        Ok(())
    }

    /// Parse and queue additional script for execution.
    ///
    /// Format:
    /// - Squares are 2 characters (e.g., "e2", "a1")
    /// - An optional `W` or `B` prefix specifies the piece color (e.g., "We4", "Be5")
    /// - Spaces separate squares in the same batch
    /// - Periods (". ") trigger a tick
    ///
    /// A color prefix is required when placing a piece on an empty square;
    /// omitting it for an occupied square infers the color from the current state.
    ///
    /// Examples:
    /// - `"e2 We4."` - Lift e2, place white on e4, then tick
    /// - `"e2.  We4."` - Lift e2, tick, place white on e4, tick
    pub fn push_script(&mut self, script: &str) -> Result<(), ParseError> {
        let batches = parse_script(script)?;
        self.pending_batches.extend(batches);
        Ok(())
    }

    /// Execute next pending batch, returning new per-color positions.
    ///
    /// Returns `Ok(None)` if no pending batches remain.
    /// Returns `Err` if a placement is attempted on an empty square without a color.
    pub fn tick(&mut self) -> Result<Option<ByColor<Bitboard>>, ParseError> {
        let Some(batch) = self.pending_batches.pop_front() else {
            return Ok(None);
        };
        for (square, color) in batch {
            self.toggle_square(square, color)?;
        }
        Ok(Some(self.positions))
    }

    /// Execute all pending batches, calling the provided callback for each.
    pub fn drain<F>(&mut self, mut on_tick: F) -> Result<(), ParseError>
    where
        F: FnMut(ByColor<Bitboard>),
    {
        while let Some(positions) = self.tick()? {
            on_tick(positions);
        }
        Ok(())
    }

    /// Toggle a square in the appropriate per-color bitboard.
    ///
    /// If the square is occupied, the color is inferred and `color` is ignored.
    /// If the square is empty, `color` must be `Some`; `None` returns [`ParseError::MissingColor`].
    fn toggle_square(&mut self, square: Square, color: Option<Color>) -> Result<(), ParseError> {
        let color = if self.positions.white.contains(square) {
            Color::White
        } else if self.positions.black.contains(square) {
            Color::Black
        } else {
            color.ok_or_else(|| ParseError::MissingColor(square.to_string()))?
        };
        self.positions[color].toggle(square);
        Ok(())
    }
}

/// Returns `Err(ParseError::OverlappingSquares)` if `white` and `black` share any square.
fn check_overlap(white: Bitboard, black: Bitboard) -> Result<(), ParseError> {
    let overlap = white & black;
    if overlap.is_empty() {
        Ok(())
    } else {
        Err(ParseError::OverlappingSquares(
            overlap
                .into_iter()
                .map(|sq| sq.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ))
    }
}

/// A parsed batch entry: a square to toggle and an optional color prefix.
type BatchEntry = (Square, Option<Color>);

/// Parse a BoardScript string into batches of `(Square, Option<Color>)` to toggle.
fn parse_script(script: &str) -> Result<Vec<Vec<BatchEntry>>, ParseError> {
    let mut batches: Vec<Vec<BatchEntry>> = vec![Vec::new()];
    let mut current_token = String::new();

    for ch in script.chars() {
        match ch {
            '.' => {
                flush_token(&mut current_token, &mut batches)?;
                batches.push(Vec::new());
            }
            c if c.is_whitespace() => {
                flush_token(&mut current_token, &mut batches)?;
            }
            _ => {
                current_token.push(ch);

                // Tokens are 2 chars for bare squares (e.g. "e2") or 3 chars
                // when prefixed with a color ('W' or 'B', e.g. "We4").
                let has_color_prefix =
                    matches!(current_token.chars().next(), Some('W') | Some('B'));
                let expected_len = if has_color_prefix { 3 } else { 2 };
                if current_token.len() == expected_len {
                    flush_token(&mut current_token, &mut batches)?;
                }
            }
        }
    }

    // Flush any remaining token
    flush_token(&mut current_token, &mut batches)?;

    // Remove empty batches
    batches.retain(|b| !b.is_empty());
    Ok(batches)
}

/// Parse the current token into a [`BatchEntry`] and clear the token.
fn flush_token(token: &mut String, batches: &mut [Vec<BatchEntry>]) -> Result<(), ParseError> {
    if !token.is_empty() {
        let (color, square_str) = match token.chars().next() {
            Some('W') => (Some(Color::White), &token[1..]),
            Some('B') => (Some(Color::Black), &token[1..]),
            _ => (None, token.as_str()),
        };
        let square: Square = square_str
            .parse()
            .map_err(|_| ParseError::InvalidSquare(token.clone()))?;
        batches
            .last_mut()
            .expect("batches should never be empty")
            .push((square, color));
        token.clear();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_error_invalid_square() {
        let mut sensor = ScriptedSensor::new();
        let result = sensor.push_script("e2.  zz.");
        assert_eq!(result, Err(ParseError::InvalidSquare("zz".to_string())));
    }

    #[test]
    fn test_parse_error_does_not_modify_state() {
        let mut sensor = ScriptedSensor::new();
        let initial = sensor.read_positions();

        // Push valid script
        sensor.push_script("e2. ").unwrap();

        // Invalid script should fail without modifying pending batches
        let result = sensor.push_script("xx.");
        assert!(result.is_err());

        // The valid batch should still be pending
        let positions = sensor.tick().unwrap();
        assert!(positions.is_some());
        assert_ne!(positions.unwrap(), initial);
    }

    #[test]
    fn test_from_bitboards_initializes_per_color() {
        let white = Bitboard::from_rank(shakmaty::Rank::First)
            | Bitboard::from_rank(shakmaty::Rank::Second);
        let black = Bitboard::from_rank(shakmaty::Rank::Seventh)
            | Bitboard::from_rank(shakmaty::Rank::Eighth);
        let sensor = ScriptedSensor::from_bitboards(white, black).unwrap();
        assert_eq!(sensor.read_positions(), ByColor { white, black });
    }

    #[test]
    fn test_from_bitboards_rejects_overlap() {
        let both = Bitboard::from_rank(shakmaty::Rank::Fourth);
        assert!(matches!(
            ScriptedSensor::from_bitboards(both, both),
            Err(ParseError::OverlappingSquares(_))
        ));
    }

    #[test]
    fn test_toggle_removes_from_white() {
        let white = Bitboard::from_rank(shakmaty::Rank::Second);
        let black = Bitboard::from_rank(shakmaty::Rank::Seventh);
        let mut sensor = ScriptedSensor::from_bitboards(white, black).unwrap();

        sensor.push_script("e2.").unwrap();
        sensor.tick().unwrap();

        assert!(!sensor.read_positions().white.contains(Square::E2));
        assert_eq!(sensor.read_positions().black, black);
    }

    #[test]
    fn test_toggle_removes_from_black() {
        let white = Bitboard::from_rank(shakmaty::Rank::Second);
        let black = Bitboard::from_rank(shakmaty::Rank::Seventh);
        let mut sensor = ScriptedSensor::from_bitboards(white, black).unwrap();

        sensor.push_script("e7.").unwrap();
        sensor.tick().unwrap();

        assert!(!sensor.read_positions().black.contains(Square::E7));
        assert_eq!(sensor.read_positions().white, white);
    }

    #[test]
    fn test_toggle_places_with_color_prefix() {
        let white = Bitboard::from_rank(shakmaty::Rank::Second);
        let black = Bitboard::from_rank(shakmaty::Rank::Seventh);
        let mut sensor = ScriptedSensor::from_bitboards(white, black).unwrap();

        sensor.push_script("We4.").unwrap();
        sensor.tick().unwrap();

        assert!(sensor.read_positions().white.contains(Square::E4));
    }

    #[test]
    fn test_tick_error_on_placement_without_color() {
        let mut sensor =
            ScriptedSensor::from_bitboards(Bitboard::EMPTY, Bitboard::EMPTY).unwrap();
        sensor.push_script("e4.").unwrap();
        assert_eq!(
            sensor.tick(),
            Err(ParseError::MissingColor("e4".to_string()))
        );
    }

    #[test]
    fn test_load_bitboards() {
        let mut sensor = ScriptedSensor::new();
        let white = Bitboard::from_rank(shakmaty::Rank::Third);
        let black = Bitboard::from_rank(shakmaty::Rank::Sixth);
        sensor.load_bitboards(white, black).unwrap();
        assert_eq!(sensor.read_positions(), ByColor { white, black });
    }

    #[test]
    fn test_load_bitboards_rejects_overlap() {
        let mut sensor = ScriptedSensor::new();
        let both = Bitboard::from_rank(shakmaty::Rank::Fourth);
        assert!(matches!(
            sensor.load_bitboards(both, both),
            Err(ParseError::OverlappingSquares(_))
        ));
    }

    #[test]
    fn test_new_matches_starting_position_colors() {
        let chess = Chess::default();
        let board = chess.board();
        let sensor = ScriptedSensor::new();
        assert_eq!(sensor.read_positions().white, board.by_color(Color::White));
        assert_eq!(sensor.read_positions().black, board.by_color(Color::Black));
    }
}
