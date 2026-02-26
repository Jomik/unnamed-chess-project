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

/// Trait for displaying board feedback to the player.
///
/// Abstracts over LED hardware (ESP32) and terminal rendering,
/// providing a uniform interface for the output side of the
/// game loop. Mirrors [`PieceSensor`] on the input side.
pub trait BoardDisplay {
    /// Error type for display update failures.
    type Error: std::fmt::Debug + std::fmt::Display;

    /// Show the given feedback state on the display.
    ///
    /// Implementations map [`feedback::SquareFeedback`] variants
    /// to hardware-specific output (LED colors, terminal colors, etc.).
    fn show(&mut self, feedback: &feedback::BoardFeedback) -> Result<(), Self::Error>;
}

#[cfg(target_os = "espidf")]
pub mod esp32;

#[cfg(not(target_os = "espidf"))]
pub mod mock;
