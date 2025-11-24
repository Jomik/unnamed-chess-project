use std::vec;

use shakmaty::{Bitboard, Chess, Piece, Position, Square};

/// Events that the game logic layer emits
#[derive(Debug, Clone, PartialEq)]
pub enum GameEvent {
    /// A piece was lifted from a square
    PieceLifted { from: shakmaty::Square },

    /// A complete move was detected
    MoveDetected {
        from: shakmaty::Square,
        to: shakmaty::Square,
    },

    /// An illegal move was attempted
    IllegalMove {
        from: shakmaty::Square,
        to: shakmaty::Square,
    },

    /// Board state doesn't match expected position
    InvalidBoardState,

    /// Game over (checkmate, stalemate, etc.)
    GameOver { result: String },
}

/// Phases that the engine can be in
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePhase {
    /// Waiting for the board to match the starting position
    Setup {
        /// Squares where pieces are missing
        missing: Bitboard,
        /// Squares where extra pieces are present
        extra: Bitboard,
    },
    /// Normal chess moves detected and processed
    Playing,
    /// Checkmate/stalemate
    GameOver,
}

/// Core game engine that processes sensor input and maintains game state
pub struct GameEngine {
    /// The logical chess position (piece types, turn, castling rights, etc.)
    position: Chess,

    /// The current game phase
    phase: GamePhase,

    /// Last known physical board state from sensors
    last_bitboard: Bitboard,

    /// Square where a piece was lifted (if any)
    lifted_from: Option<Square>,
}

impl GameEngine {
    pub fn new() -> Self {
        Self {
            position: Chess::default(),
            phase: GamePhase::Setup {
                missing: Bitboard::EMPTY,
                extra: Bitboard::EMPTY,
            },
            last_bitboard: Bitboard::EMPTY,
            lifted_from: None,
        }
    }

    /// Get the current game phase
    pub fn phase(&self) -> GamePhase {
        self.phase
    }

    /// Get the piece at a given square, if any
    pub fn piece_at(&self, square: Square) -> Option<Piece> {
        self.position.board().piece_at(square)
    }

    /// Get the square where a piece was last lifted from
    pub fn lifted_from(&self) -> Option<Square> {
        self.lifted_from
    }

    /// Process a board state reading and return any events that occurred
    ///
    /// The engine tracks changes and returns events describing what happened.
    pub fn tick(&mut self, current_bb: Bitboard) -> Vec<GameEvent> {
        if current_bb == self.last_bitboard {
            return vec![]; // Physical board hasn't changed
        }

        let events = match self.phase {
            GamePhase::Setup { .. } => self.process_setup(current_bb),
            GamePhase::Playing => self.process_playing(current_bb),
            GamePhase::GameOver => self.process_game_over(current_bb),
        };
        self.last_bitboard = current_bb;
        events
    }

    fn process_setup(&mut self, current_bb: Bitboard) -> Vec<GameEvent> {
        let expected = Chess::default().board().occupied();

        if current_bb == expected {
            self.phase = GamePhase::Playing;
            vec![]
        } else {
            let missing = expected & !current_bb;
            let extra = current_bb & !expected;
            self.phase = GamePhase::Setup { missing, extra };
            vec![]
        }
    }

    fn process_playing(&mut self, current_bb: Bitboard) -> Vec<GameEvent> {
        let removed = self.last_bitboard & !current_bb;
        let added = current_bb & !self.last_bitboard;

        match (removed.count(), added.count()) {
            (0, 0) => vec![],
            (1, 0) => {
                let from = removed.first().unwrap();
                self.lifted_from = Some(from);
                vec![GameEvent::PieceLifted { from }]
            }
            (0, 1) => {
                let to = added.first().unwrap();
                if let Some(from) = self.lifted_from.take() {
                    self.handle_move(from, to)
                } else {
                    vec![GameEvent::InvalidBoardState]
                }
            }
            (1, 1) => {
                let from = removed.first().unwrap();
                let to = added.first().unwrap();
                self.handle_move(from, to)
            }
            _ => vec![GameEvent::InvalidBoardState],
        }
    }

    fn process_game_over(&mut self, _current_bb: Bitboard) -> Vec<GameEvent> {
        if self.position.is_game_over() {
            self.phase = GamePhase::GameOver;
        }
        vec![]
    }

    fn handle_move(&mut self, from: Square, to: Square) -> Vec<GameEvent> {
        let legal = self
            .position
            .legal_moves()
            .into_iter()
            .find(|m| m.from() == Some(from) && m.to() == to);
        if let Some(mv) = legal {
            // Play the move and update the position
            self.position.play_unchecked(mv);
            vec![GameEvent::MoveDetected { from, to }]
        } else {
            vec![GameEvent::IllegalMove { from, to }]
        }
    }
}

impl Default for GameEngine {
    fn default() -> Self {
        Self::new()
    }
}
