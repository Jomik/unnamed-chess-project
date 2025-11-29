use std::collections::VecDeque;

use shakmaty::{Bitboard, Chess, Position, Square};
use thiserror::Error;

/// Error when parsing a board script.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("invalid square notation: '{0}'")]
pub struct ParseError(String);

/// A scriptable mock sensor that processes BoardScript format.
///
/// Maintains bitboard state and executes script batches on demand.
/// New script can be appended at any time for interactive use.
#[derive(Debug, Clone)]
pub struct ScriptedSensor {
    bitboard: Bitboard,
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
        Self::from_bitboard(Chess::default().board().occupied())
    }

    /// Create from a specific bitboard state.
    pub fn from_bitboard(bitboard: Bitboard) -> Self {
        Self {
            bitboard,
            pending_batches: VecDeque::new(),
        }
    }

    /// Current sensor reading.
    #[inline]
    pub fn read_positions(&self) -> Bitboard {
        self.bitboard
    }

    /// Load a new bitboard directly (for FEN loading).
    pub fn load_bitboard(&mut self, bitboard: Bitboard) {
        self.bitboard = bitboard;
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

    /// Execute next pending batch, returning new bitboard state.
    /// Returns None if no pending batches.
    pub fn tick(&mut self) -> Option<Bitboard> {
        let batch = self.pending_batches.pop_front()?;
        for square in batch {
            self.bitboard.toggle(square);
        }
        Some(self.bitboard)
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
}
