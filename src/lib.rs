use shakmaty::{Bitboard, ByColor};

pub mod feedback;
pub mod game_logic;

/// Trait for reading piece positions from the board.
///
/// Abstracts over hardware sensors (ESP32) and mock/scripted inputs,
/// providing a uniform interface for `GameEngine`.
pub trait PieceSensor {
    /// Error type for sensor read failures.
    type Error: std::fmt::Debug + std::fmt::Display;

    /// Read current per-color piece positions from the board.
    fn read_positions(&mut self) -> Result<ByColor<Bitboard>, Self::Error>;
}

#[cfg(target_os = "espidf")]
pub mod esp32;

#[cfg(not(target_os = "espidf"))]
pub mod mock;
