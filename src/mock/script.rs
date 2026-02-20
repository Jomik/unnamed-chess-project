use std::collections::VecDeque;

use shakmaty::{Bitboard, Chess, Color, Position, Square};
use thiserror::Error;

/// Error when parsing a board script.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("invalid square notation: '{0}'")]
pub struct ParseError(String);

/// A scriptable mock sensor that processes BoardScript format.
///
/// Maintains per-color bitboard state and executes script batches on demand.
/// New script can be appended at any time for interactive use.
#[derive(Debug, Clone)]
pub struct ScriptedSensor {
    white_bb: Bitboard,
    black_bb: Bitboard,
    pending_batches: VecDeque<Vec<Square>>,
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
    }

    /// Create from a combined bitboard (color information unknown; all squares assigned to white).
    pub fn from_bitboard(bitboard: Bitboard) -> Self {
        Self::from_bitboards(bitboard, Bitboard::EMPTY)
    }

    /// Create from separate white and black bitboards.
    pub fn from_bitboards(white_bb: Bitboard, black_bb: Bitboard) -> Self {
        Self {
            white_bb,
            black_bb,
            pending_batches: VecDeque::new(),
        }
    }

    /// Combined occupancy of both colors.
    #[inline]
    pub fn read_positions(&self) -> Bitboard {
        self.white_bb | self.black_bb
    }

    /// White piece positions.
    #[inline]
    pub fn white_bb(&self) -> Bitboard {
        self.white_bb
    }

    /// Black piece positions.
    #[inline]
    pub fn black_bb(&self) -> Bitboard {
        self.black_bb
    }

    /// Load a new combined bitboard (color information unknown; all squares assigned to white).
    pub fn load_bitboard(&mut self, bitboard: Bitboard) {
        self.load_bitboards(bitboard, Bitboard::EMPTY);
    }

    /// Load separate white and black bitboards directly (e.g. when loading a FEN position).
    pub fn load_bitboards(&mut self, white_bb: Bitboard, black_bb: Bitboard) {
        self.white_bb = white_bb;
        self.black_bb = black_bb;
        self.pending_batches.clear();
    }

    /// Parse and queue additional script for execution.
    ///
    /// Format:
    /// - Squares are 2 characters (e.g., "e2", "a1")
    /// - Spaces separate squares in the same batch
    /// - Periods (". ") trigger a tick
    ///
    /// Examples:
    /// - `"e2e4."` - Toggle e2 & e4 together, then tick
    /// - `"e2 e4."` - Same (explicit space)
    /// - `"e2.  e4."` - Toggle e2, tick, toggle e4, tick
    pub fn push_script(&mut self, script: &str) -> Result<(), ParseError> {
        let batches = parse_script(script)?;
        self.pending_batches.extend(batches);
        Ok(())
    }

    /// Execute next pending batch, returning new combined occupancy.
    /// Returns None if no pending batches.
    pub fn tick(&mut self) -> Option<Bitboard> {
        let batch = self.pending_batches.pop_front()?;
        for square in batch {
            self.toggle_square(square);
        }
        Some(self.white_bb | self.black_bb)
    }

    /// Execute all pending batches, calling the provided callback for each.
    pub fn drain<F>(&mut self, mut on_tick: F)
    where
        F: FnMut(Bitboard),
    {
        while let Some(bb) = self.tick() {
            on_tick(bb);
        }
    }

    /// Toggle a square in the appropriate per-color bitboard.
    ///
    /// Removes from whichever color currently occupies the square.
    /// If the square is unoccupied (adding a piece without known color),
    /// it is added to `white_bb` as a fallback.
    fn toggle_square(&mut self, square: Square) {
        if self.white_bb.contains(square) {
            self.white_bb.toggle(square);
        } else if self.black_bb.contains(square) {
            self.black_bb.toggle(square);
        } else {
            // Piece placed with no known color; assign to white as fallback.
            self.white_bb.toggle(square);
        }
    }
}

/// Parse a BoardScript string into batches of squares to toggle.
fn parse_script(script: &str) -> Result<Vec<Vec<Square>>, ParseError> {
    let mut batches: Vec<Vec<Square>> = vec![Vec::new()];
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

                // Squares are exactly 2 characters (e.g., "e2", "a1")
                if current_token.len() == 2 {
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

/// Add current token to the last batch and clear it.
fn flush_token(token: &mut String, batches: &mut [Vec<Square>]) -> Result<(), ParseError> {
    if !token.is_empty() {
        let square: Square = token
            .trim()
            .parse()
            .map_err(|_| ParseError(token.clone()))?;
        batches
            .last_mut()
            .expect("batches should never be empty")
            .push(square);
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
        assert_eq!(result, Err(ParseError("zz".to_string())));
    }

    #[test]
    fn test_parse_error_does_not_modify_state() {
        let mut sensor = ScriptedSensor::new();
        let initial_bb = sensor.read_positions();

        // Push valid script
        sensor.push_script("e2. ").unwrap();

        // Invalid script should fail without modifying pending batches
        let result = sensor.push_script("xx.");
        assert!(result.is_err());

        // The valid batch should still be pending
        let bb = sensor.tick();
        assert!(bb.is_some());
        assert_ne!(bb.unwrap(), initial_bb);
    }

    #[test]
    fn test_from_bitboards_initializes_per_color() {
        let white = Bitboard::from_rank(shakmaty::Rank::First)
            | Bitboard::from_rank(shakmaty::Rank::Second);
        let black = Bitboard::from_rank(shakmaty::Rank::Seventh)
            | Bitboard::from_rank(shakmaty::Rank::Eighth);
        let sensor = ScriptedSensor::from_bitboards(white, black);
        assert_eq!(sensor.white_bb(), white);
        assert_eq!(sensor.black_bb(), black);
        assert_eq!(sensor.read_positions(), white | black);
    }

    #[test]
    fn test_toggle_removes_from_white() {
        let white = Bitboard::from_rank(shakmaty::Rank::Second);
        let black = Bitboard::from_rank(shakmaty::Rank::Seventh);
        let mut sensor = ScriptedSensor::from_bitboards(white, black);

        sensor.push_script("e2.").unwrap();
        sensor.tick();

        assert!(!sensor.white_bb().contains(Square::E2));
        assert_eq!(sensor.black_bb(), black);
    }

    #[test]
    fn test_toggle_removes_from_black() {
        let white = Bitboard::from_rank(shakmaty::Rank::Second);
        let black = Bitboard::from_rank(shakmaty::Rank::Seventh);
        let mut sensor = ScriptedSensor::from_bitboards(white, black);

        sensor.push_script("e7.").unwrap();
        sensor.tick();

        assert!(!sensor.black_bb().contains(Square::E7));
        assert_eq!(sensor.white_bb(), white);
    }

    #[test]
    fn test_load_bitboards() {
        let mut sensor = ScriptedSensor::new();
        let white = Bitboard::from_rank(shakmaty::Rank::Third);
        let black = Bitboard::from_rank(shakmaty::Rank::Sixth);
        sensor.load_bitboards(white, black);
        assert_eq!(sensor.white_bb(), white);
        assert_eq!(sensor.black_bb(), black);
        assert_eq!(sensor.read_positions(), white | black);
    }

    #[test]
    fn test_new_matches_starting_position_colors() {
        let chess = Chess::default();
        let board = chess.board();
        let sensor = ScriptedSensor::new();
        assert_eq!(sensor.white_bb(), board.by_color(Color::White));
        assert_eq!(sensor.black_bb(), board.by_color(Color::Black));
    }
}
