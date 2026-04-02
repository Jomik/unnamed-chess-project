mod embedded;
mod human;

pub use embedded::EmbeddedEngine;
pub use human::HumanPlayer;

use shakmaty::{Bitboard, ByColor, Chess, Move};

/// Player health status, checked by the session each tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerStatus {
    /// Normal operation.
    Active,
    /// Unrecoverable error (e.g. Lichess disconnect).
    Error,
    /// External game termination (e.g. Lichess resign/timeout).
    GameOver,
}

/// A chess player — either human (detecting moves from sensors) or computer (computing moves).
pub trait Player {
    /// Return a move if one is detected/ready. Called every tick for the active player.
    ///
    /// `position` is the current chess position (owned by the session).
    /// `sensors` is the current physical board state from hall-effect sensors.
    /// Computer players ignore `sensors`.
    fn poll_move(&mut self, position: &Chess, sensors: ByColor<Bitboard>) -> Option<Move>;

    /// Notification that the opponent just played.
    ///
    /// `position` is the post-move state. Override for async players (e.g. Lichess)
    /// that need to send the move to an external service.
    /// Default is a no-op.
    fn opponent_moved(&mut self, _position: &Chess, _opponent_move: &Move) {}

    /// Player health. Checked by session each tick.
    fn status(&self) -> PlayerStatus {
        PlayerStatus::Active
    }

    /// Whether this player interacts physically with the board.
    ///
    /// Interactive players (humans) trigger move guidance when lifting pieces.
    /// Non-interactive players (engines, Lichess) suppress guidance during their turn.
    fn is_interactive(&self) -> bool {
        true
    }
}
