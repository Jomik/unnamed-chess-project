use shakmaty::Color;

/// The game lifecycle state.
///
/// Defined in `docs/board-api.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameStatus {
    /// No game in progress.
    Idle,
    /// Waiting for starting position on sensors.
    AwaitingPieces,
    /// Game is actively being played.
    InProgress,
    /// Game ended by checkmate.
    Checkmate { loser: Color },
    /// Game ended by stalemate.
    Stalemate,
    /// A player resigned.
    Resigned { color: Color },
}

impl GameStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            GameStatus::Checkmate { .. } | GameStatus::Stalemate | GameStatus::Resigned { .. }
        )
    }
}

/// Determines how moves arrive for a given side.
///
/// Defined in `docs/board-api.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerType {
    /// Detected from sensors on the physical board.
    Human,
    /// Delivered via SubmitMove.
    Remote,
}

/// Typed errors for Board API operations.
///
/// Defined in `docs/board-api.md`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BoardApiError {
    #[error("game already in progress")]
    GameAlreadyInProgress,
    #[error("no game in progress")]
    NoGameInProgress,
    #[error("not your turn")]
    NotYourTurn,
    #[error("illegal move")]
    IllegalMove,
    #[error("cannot resign for remote player")]
    CannotResignForRemotePlayer,
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::Color;

    #[test]
    fn idle_is_not_terminal() {
        assert!(!GameStatus::Idle.is_terminal());
    }

    #[test]
    fn awaiting_pieces_is_not_terminal() {
        assert!(!GameStatus::AwaitingPieces.is_terminal());
    }

    #[test]
    fn in_progress_is_not_terminal() {
        assert!(!GameStatus::InProgress.is_terminal());
    }

    #[test]
    fn checkmate_is_terminal() {
        assert!(
            GameStatus::Checkmate {
                loser: Color::White
            }
            .is_terminal()
        );
    }

    #[test]
    fn stalemate_is_terminal() {
        assert!(GameStatus::Stalemate.is_terminal());
    }

    #[test]
    fn resigned_is_terminal() {
        assert!(
            GameStatus::Resigned {
                color: Color::Black
            }
            .is_terminal()
        );
    }

    #[test]
    fn checkmate_carries_loser() {
        let status = GameStatus::Checkmate {
            loser: Color::Black,
        };
        assert_eq!(
            status,
            GameStatus::Checkmate {
                loser: Color::Black
            }
        );
        assert_ne!(
            status,
            GameStatus::Checkmate {
                loser: Color::White
            }
        );
    }

    #[test]
    fn resigned_carries_color() {
        let status = GameStatus::Resigned {
            color: Color::White,
        };
        assert_eq!(
            status,
            GameStatus::Resigned {
                color: Color::White
            }
        );
        assert_ne!(
            status,
            GameStatus::Resigned {
                color: Color::Black
            }
        );
    }

    #[test]
    fn player_type_debug() {
        assert_eq!(format!("{:?}", PlayerType::Human), "Human");
        assert_eq!(format!("{:?}", PlayerType::Remote), "Remote");
    }

    #[test]
    fn board_api_error_display() {
        assert_eq!(
            BoardApiError::GameAlreadyInProgress.to_string(),
            "game already in progress"
        );
        assert_eq!(
            BoardApiError::NoGameInProgress.to_string(),
            "no game in progress"
        );
        assert_eq!(BoardApiError::NotYourTurn.to_string(), "not your turn");
        assert_eq!(BoardApiError::IllegalMove.to_string(), "illegal move");
        assert_eq!(
            BoardApiError::CannotResignForRemotePlayer.to_string(),
            "cannot resign for remote player"
        );
    }
}
