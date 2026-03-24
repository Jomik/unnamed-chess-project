use shakmaty::Color;

/// Configuration for Lichess integration, constructed from compile-time env vars.
/// The token is not included here — it is passed directly to the LichessClient
/// constructor, keeping the secret out of a general config struct.
pub struct LichessConfig {
    pub level: u8,
    pub clock_limit: u32,
    pub clock_increment: u32,
}

/// A single game event from the Lichess NDJSON stream.
pub enum GameEvent {
    GameFull {
        id: String,
        initial_fen: String,
        state: GameStateData,
    },
    GameState(GameStateData),
}

/// The mutable state portion of a game event.
pub struct GameStateData {
    pub moves: String,
    pub status: GameStatus,
    pub winner: Option<Color>,
}

/// Game status from the Lichess API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameStatus {
    Started,
    Mate,
    Resign,
    Stalemate,
    Draw,
    Aborted,
    Timeout,
    Outoftime,
    Other(String),
}

impl GameStatus {
    /// Parse a status string from the Lichess API.
    pub fn from_str(s: &str) -> Self {
        match s {
            "started" | "created" => Self::Started,
            "mate" => Self::Mate,
            "resign" => Self::Resign,
            "stalemate" => Self::Stalemate,
            "draw" => Self::Draw,
            "aborted" => Self::Aborted,
            "timeout" => Self::Timeout,
            "outoftime" => Self::Outoftime,
            other => Self::Other(other.to_string()),
        }
    }

    /// Whether this status represents a terminal (game-over) state.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Started)
    }
}

/// Messages sent from the background worker to `LichessOpponent`.
pub enum LichessMessage {
    AiMove(String),
    GameOver,
    Error(String),
}

/// Errors from `spawn_lichess_opponent`.
pub enum SpawnError<H, S> {
    Http(H),
    Spawn(S),
    WorkerStartup(String),
}

impl<H: std::fmt::Display, S: std::fmt::Display> std::fmt::Display for SpawnError<H, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "challenge_ai failed: {e}"),
            Self::Spawn(e) => write!(f, "worker spawn failed: {e}"),
            Self::WorkerStartup(e) => write!(f, "worker startup failed: {e}"),
        }
    }
}

impl<H: std::fmt::Debug, S: std::fmt::Debug> std::fmt::Debug for SpawnError<H, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "SpawnError::Http({e:?})"),
            Self::Spawn(e) => write!(f, "SpawnError::Spawn({e:?})"),
            Self::WorkerStartup(e) => write!(f, "SpawnError::WorkerStartup({e:?})"),
        }
    }
}

/// Used on the calling thread during startup. Implementations may be !Send.
pub trait LichessClient {
    type Error: std::fmt::Debug + std::fmt::Display;
    type Game: LichessGame<Error = Self::Error> + Send + 'static;

    fn challenge_ai(
        self,
        level: u8,
        clock_limit: u32,
        clock_increment: u32,
    ) -> Result<Self::Game, Self::Error>;
}

/// Moved into the background worker. Must be Send.
/// Holds game ID and config — no HTTP connection.
pub trait LichessGame: Send + 'static {
    type Error: std::fmt::Debug + std::fmt::Display;

    fn game_id(&self) -> &str;
    fn into_stream(self) -> Result<Box<dyn LichessStream<Error = Self::Error>>, Self::Error>;
}

/// Owned by the background worker. May be !Send.
pub trait LichessStream {
    type Error: std::fmt::Debug + std::fmt::Display;

    fn next_event(&mut self) -> Option<Result<GameEvent, Self::Error>>;
    fn make_move(&mut self, uci_move: &str) -> Result<(), Self::Error>;
}
