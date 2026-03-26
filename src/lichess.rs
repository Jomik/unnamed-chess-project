use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::time::Duration;

use shakmaty::uci::UciMove;
use shakmaty::{Chess, Color, Move};

use crate::player::{Player, PlayerStatus};

/// Timeout for worker startup. Must be longer than the stream connection timeout
/// (60s in esp32/lichess.rs) to avoid false failures on slow networks.
const WORKER_STARTUP_TIMEOUT: Duration = Duration::from_secs(90);

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
    OutOfTime,
    Other(String),
}

impl GameStatus {
    /// Parse a status string from the Lichess API.
    pub fn parse(s: &str) -> Self {
        match s {
            "started" | "created" => Self::Started,
            "mate" => Self::Mate,
            "resign" => Self::Resign,
            "stalemate" => Self::Stalemate,
            "draw" => Self::Draw,
            "aborted" => Self::Aborted,
            "timeout" => Self::Timeout,
            "outoftime" => Self::OutOfTime,
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

/// Creates a Lichess game challenge. Used on the calling thread during startup.
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

/// A created game, moved into the background worker to open the event stream.
pub trait LichessGame: Send + 'static {
    type Error: std::fmt::Debug + std::fmt::Display;

    fn game_id(&self) -> &str;
    fn into_stream(self) -> Result<Box<dyn LichessStream<Error = Self::Error>>, Self::Error>;
}

/// Owned by the background worker.
pub trait LichessStream {
    type Error: std::fmt::Debug + std::fmt::Display;

    fn next_event(&mut self) -> Option<Result<GameEvent, Self::Error>>;
    fn make_move(&mut self, uci_move: &str) -> Result<(), Self::Error>;
}

/// Lichess opponent that communicates with a background worker via channels.
#[derive(Debug)]
pub struct LichessOpponent {
    player_move_tx: SyncSender<String>,
    ai_move_rx: Receiver<LichessMessage>,
    error: bool,
    game_over: bool,
}

impl Player for LichessOpponent {
    fn opponent_moved(&mut self, _position: &Chess, opponent_move: &Move) {
        if self.error || self.game_over {
            return;
        }
        let uci = UciMove::from_standard(*opponent_move).to_string();
        match self.player_move_tx.try_send(uci) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(_)) => {
                log::warn!("Lichess worker busy, dropping move");
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                log::warn!("Lichess worker disconnected");
                self.error = true;
            }
        }
    }

    fn poll_move(
        &mut self,
        position: &Chess,
        _sensors: shakmaty::ByColor<shakmaty::Bitboard>,
    ) -> Option<Move> {
        if self.error || self.game_over {
            return None;
        }
        match self.ai_move_rx.try_recv() {
            Ok(LichessMessage::AiMove(uci_str)) => {
                match UciMove::from_str(&uci_str)
                    .ok()
                    .and_then(|u| u.to_move(position).ok())
                {
                    Some(mv) => Some(mv),
                    None => {
                        log::warn!("Invalid UCI from Lichess: {uci_str}");
                        self.error = true;
                        None
                    }
                }
            }
            Ok(LichessMessage::GameOver) => {
                log::info!("Lichess game over");
                self.game_over = true;
                None
            }
            Ok(LichessMessage::Error(e)) => {
                log::warn!("Lichess error: {e}");
                self.error = true;
                None
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                if self.game_over {
                    None
                } else {
                    log::warn!("Lichess worker disconnected unexpectedly");
                    self.error = true;
                    None
                }
            }
        }
    }

    fn status(&self) -> PlayerStatus {
        if self.error {
            PlayerStatus::Error
        } else if self.game_over {
            PlayerStatus::GameOver
        } else {
            PlayerStatus::Active
        }
    }
}

/// Create a Lichess game, spawn a background worker, and return a `LichessOpponent`.
///
/// Blocks until the worker signals readiness (i.e. the initial `GameFull` event
/// has been received from the stream).
pub fn spawn_lichess_opponent<C, E>(
    client: C,
    config: LichessConfig,
    spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> Result<(), E>,
) -> Result<LichessOpponent, SpawnError<C::Error, E>>
where
    C: LichessClient,
    E: std::fmt::Debug + std::fmt::Display,
{
    let game = client
        .challenge_ai(config.level, config.clock_limit, config.clock_increment)
        .map_err(SpawnError::Http)?;

    log::info!("Lichess game created: {}", game.game_id());

    let (player_tx, player_rx) = mpsc::sync_channel::<String>(1);
    let (ai_tx, ai_rx) = mpsc::sync_channel::<LichessMessage>(1);
    let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);

    let worker = Box::new(move || {
        worker_loop(game, player_rx, ai_tx, ready_tx);
    });

    spawn(worker).map_err(SpawnError::Spawn)?;

    match ready_rx.recv_timeout(WORKER_STARTUP_TIMEOUT) {
        Ok(Ok(())) => Ok(LichessOpponent {
            player_move_tx: player_tx,
            ai_move_rx: ai_rx,
            error: false,
            game_over: false,
        }),
        Ok(Err(desc)) => Err(SpawnError::WorkerStartup(desc)),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err(SpawnError::WorkerStartup("worker startup timed out".into()))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(SpawnError::WorkerStartup(
            "worker exited before signalling ready".into(),
        )),
    }
}

/// Background worker loop that bridges the Lichess stream and the opponent channels.
fn worker_loop<G: LichessGame>(
    game: G,
    player_rx: mpsc::Receiver<String>,
    ai_tx: mpsc::SyncSender<LichessMessage>,
    ready_tx: mpsc::SyncSender<Result<(), String>>,
) {
    let mut stream = match game.into_stream() {
        Ok(s) => s,
        Err(e) => {
            let _ = ready_tx.send(Err(format!("into_stream failed: {e}")));
            return;
        }
    };

    let initial_move_count = match stream.next_event() {
        Some(Ok(GameEvent::GameFull { state, .. })) => state.moves.split_whitespace().count(),
        Some(Ok(_)) => {
            let _ = ready_tx.send(Err("expected GameFull, got GameState".into()));
            return;
        }
        Some(Err(e)) => {
            let _ = ready_tx.send(Err(format!("stream error: {e}")));
            return;
        }
        None => {
            let _ = ready_tx.send(Err("stream closed before GameFull".into()));
            return;
        }
    };

    let _ = ready_tx.send(Ok(()));

    let mut expected_move_count = initial_move_count;

    loop {
        let player_uci = match player_rx.recv() {
            Ok(uci) => uci,
            Err(_) => return,
        };

        if let Err(e) = stream.make_move(&player_uci) {
            let _ = ai_tx.send(LichessMessage::Error(format!("make_move failed: {e}")));
            return;
        }

        expected_move_count += 2;

        loop {
            match stream.next_event() {
                Some(Ok(GameEvent::GameState(state)))
                | Some(Ok(GameEvent::GameFull { state, .. })) => {
                    let move_count = state.moves.split_whitespace().count();

                    if move_count >= expected_move_count {
                        let ai_uci = state
                            .moves
                            .split_whitespace()
                            .last()
                            .unwrap_or_default()
                            .to_string();
                        let _ = ai_tx.send(LichessMessage::AiMove(ai_uci));

                        if state.status.is_terminal() {
                            let _ = ai_tx.send(LichessMessage::GameOver);
                            return;
                        }
                        break;
                    } else if state.status.is_terminal() {
                        let _ = ai_tx.send(LichessMessage::GameOver);
                        return;
                    }
                }
                Some(Err(e)) => {
                    let _ = ai_tx.send(LichessMessage::Error(format!("stream error: {e}")));
                    return;
                }
                None => {
                    let _ = ai_tx.send(LichessMessage::Error("stream closed unexpectedly".into()));
                    return;
                }
            }
        }
    }
}

// --- JSON parsing helpers (platform-independent) ---
// These are used by the ESP32 HTTP implementation to parse Lichess NDJSON
// events without pulling in a JSON crate. Kept here so they're testable
// on the host. On non-ESP32 builds they're only used by tests.

/// Extract a JSON string value by key. Finds `"key":"value"` or `"key": "value"`.
#[cfg_attr(not(target_os = "espidf"), allow(dead_code))]
pub(crate) fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let start = json.find(&pattern)? + pattern.len();
    let rest = &json[start..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Extract a nested JSON object as a raw string by brace-matching.
#[cfg_attr(not(target_os = "espidf"), allow(dead_code))]
pub(crate) fn extract_json_object(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let key_start = json.find(&pattern)?;
    let after_key = &json[key_start + pattern.len()..];
    let after_key = after_key.trim_start();
    let after_key = after_key.strip_prefix(':')?;
    let after_key = after_key.trim_start();

    if !after_key.starts_with('{') {
        return None;
    }

    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;
    for (i, ch) in after_key.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(after_key[..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a Lichess NDJSON line into a GameEvent.
/// Returns `None` for unknown event types (silently skipped),
/// `Some(Ok(...))` for parsed events, `Some(Err(...))` for parse failures.
#[cfg_attr(not(target_os = "espidf"), allow(dead_code))]
pub(crate) fn parse_game_event(line: &str) -> Option<Result<GameEvent, String>> {
    let event_type = match extract_json_string(line, "type") {
        Some(t) => t,
        None => {
            return Some(Err(format!("no 'type' in event: {line}")));
        }
    };

    match event_type.as_str() {
        "gameFull" => {
            let id = extract_json_string(line, "id").unwrap_or_default();
            let initial_fen =
                extract_json_string(line, "initialFen").unwrap_or_else(|| "startpos".to_string());
            let state_json = match extract_json_object(line, "state") {
                Some(s) => s,
                None => {
                    return Some(Err("no 'state' object in gameFull".into()));
                }
            };
            let state = match parse_state_data(&state_json) {
                Ok(s) => s,
                Err(e) => return Some(Err(e)),
            };
            Some(Ok(GameEvent::GameFull {
                id,
                initial_fen,
                state,
            }))
        }
        "gameState" => Some(parse_state_data(line).map(GameEvent::GameState)),
        _ => {
            log::debug!("Ignoring Lichess event type: {event_type}");
            None
        }
    }
}

/// Parse the state portion of a Lichess game event.
#[cfg_attr(not(target_os = "espidf"), allow(dead_code))]
pub(crate) fn parse_state_data(json: &str) -> Result<GameStateData, String> {
    let moves = extract_json_string(json, "moves").unwrap_or_default();
    let status_str =
        extract_json_string(json, "status").ok_or_else(|| "no 'status' field".to_string())?;
    let winner = extract_json_string(json, "winner").and_then(|w| match w.as_str() {
        "white" => Some(shakmaty::Color::White),
        "black" => Some(shakmaty::Color::Black),
        _ => None,
    });

    Ok(GameStateData {
        moves,
        status: GameStatus::parse(&status_str),
        winner,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::uci::UciMove;
    use shakmaty::{Bitboard, ByColor, Chess, Position};
    use std::str::FromStr;

    fn empty_sensors() -> ByColor<Bitboard> {
        ByColor {
            white: Bitboard::EMPTY,
            black: Bitboard::EMPTY,
        }
    }

    struct MockStream {
        events: Vec<Result<GameEvent, String>>,
        moves_received: Vec<String>,
    }

    impl LichessStream for MockStream {
        type Error = String;

        fn next_event(&mut self) -> Option<Result<GameEvent, Self::Error>> {
            if self.events.is_empty() {
                None
            } else {
                Some(self.events.remove(0))
            }
        }

        fn make_move(&mut self, uci_move: &str) -> Result<(), Self::Error> {
            self.moves_received.push(uci_move.to_string());
            Ok(())
        }
    }

    struct MockGame {
        game_id: String,
        events: Vec<Result<GameEvent, String>>,
    }

    impl LichessGame for MockGame {
        type Error = String;

        fn game_id(&self) -> &str {
            &self.game_id
        }

        fn into_stream(self) -> Result<Box<dyn LichessStream<Error = String>>, String> {
            Ok(Box::new(MockStream {
                events: self.events,
                moves_received: vec![],
            }))
        }
    }

    struct MockClient {
        game: Option<MockGame>,
    }

    impl LichessClient for MockClient {
        type Error = String;
        type Game = MockGame;

        fn challenge_ai(
            self,
            _level: u8,
            _clock_limit: u32,
            _clock_increment: u32,
        ) -> Result<MockGame, String> {
            self.game
                .ok_or_else(|| "mock: no game configured".to_string())
        }
    }

    fn spawn_thread(f: Box<dyn FnOnce() + Send>) -> Result<(), String> {
        std::thread::spawn(f);
        Ok(())
    }

    fn default_config() -> LichessConfig {
        LichessConfig {
            level: 1,
            clock_limit: 300,
            clock_increment: 0,
        }
    }

    #[test]
    fn spawn_and_receive_ai_move() {
        // GameFull with empty moves (game just started), then after human plays
        // e2e4 the AI replies with e7e5
        let events = vec![
            Ok(GameEvent::GameFull {
                id: "test1".into(),
                initial_fen: "startpos".into(),
                state: GameStateData {
                    moves: String::new(),
                    status: GameStatus::Started,
                    winner: None,
                },
            }),
            // After human's e2e4, AI responds with e7e5
            Ok(GameEvent::GameState(GameStateData {
                moves: "e2e4 e7e5".into(),
                status: GameStatus::Started,
                winner: None,
            })),
        ];

        let client = MockClient {
            game: Some(MockGame {
                game_id: "test1".into(),
                events,
            }),
        };

        let mut opponent =
            spawn_lichess_opponent(client, default_config(), spawn_thread).expect("should spawn");

        // Simulate the human playing e2-e4
        let position = Chess::default();
        let human_uci = UciMove::from_str("e2e4").unwrap();
        let human_move = human_uci.to_move(&position).unwrap();

        opponent.opponent_moved(&position, &human_move);

        // Poll until the AI move arrives (with timeout)
        let after_human = {
            let mut pos = position.clone();
            pos.play_unchecked(human_move);
            pos
        };

        let mut ai_move = None;
        for _ in 0..100 {
            ai_move = opponent.poll_move(&after_human, empty_sensors());
            if ai_move.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let ai_move = ai_move.expect("should receive AI move");
        let ai_uci = UciMove::from_standard(ai_move).to_string();
        assert_eq!(ai_uci, "e7e5");
        assert!(opponent.status() != PlayerStatus::Error);
    }

    #[test]
    fn spawn_fails_when_challenge_fails() {
        let client = MockClient { game: None };

        let result = spawn_lichess_opponent(client, default_config(), spawn_thread);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, SpawnError::Http(_)),
            "expected Http error, got: {err:?}"
        );
    }

    #[test]
    fn worker_signals_error_on_stream_close() {
        // GameFull then stream closes immediately (no more events)
        let events = vec![Ok(GameEvent::GameFull {
            id: "test2".into(),
            initial_fen: "startpos".into(),
            state: GameStateData {
                moves: String::new(),
                status: GameStatus::Started,
                winner: None,
            },
        })];

        let client = MockClient {
            game: Some(MockGame {
                game_id: "test2".into(),
                events,
            }),
        };

        let mut opponent =
            spawn_lichess_opponent(client, default_config(), spawn_thread).expect("should spawn");

        // Send a human move — the worker will try to read the next event and find the stream closed
        let position = Chess::default();
        let human_uci = UciMove::from_str("e2e4").unwrap();
        let human_move = human_uci.to_move(&position).unwrap();

        opponent.opponent_moved(&position, &human_move);

        let after_human = {
            let mut pos = position.clone();
            pos.play_unchecked(human_move);
            pos
        };

        // Poll until error is detected
        for _ in 0..100 {
            let _ = opponent.poll_move(&after_human, empty_sensors());
            if opponent.status() == PlayerStatus::Error {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(opponent.status() == PlayerStatus::Error);
    }

    #[test]
    fn game_over_after_checkmate() {
        // Scholar's mate scenario: AI delivers checkmate
        let events = vec![
            Ok(GameEvent::GameFull {
                id: "test3".into(),
                initial_fen: "startpos".into(),
                state: GameStateData {
                    moves: String::new(),
                    status: GameStatus::Started,
                    winner: None,
                },
            }),
            // After human plays e2e4, AI replies e7e5 (game continues)
            Ok(GameEvent::GameState(GameStateData {
                moves: "e2e4 e7e5".into(),
                status: GameStatus::Started,
                winner: None,
            })),
            // After human plays d1h5, AI replies with checkmate
            Ok(GameEvent::GameState(GameStateData {
                moves: "e2e4 e7e5 d1h5 a7a6".into(),
                status: GameStatus::Mate,
                winner: Some(Color::Black),
            })),
        ];

        let client = MockClient {
            game: Some(MockGame {
                game_id: "test3".into(),
                events,
            }),
        };

        let mut opponent =
            spawn_lichess_opponent(client, default_config(), spawn_thread).expect("should spawn");

        // Move 1: human plays e2-e4
        let pos0 = Chess::default();
        let human1_uci = UciMove::from_str("e2e4").unwrap();
        let human1_move = human1_uci.to_move(&pos0).unwrap();
        opponent.opponent_moved(&pos0, &human1_move);

        let pos1 = {
            let mut p = pos0.clone();
            p.play_unchecked(human1_move);
            p
        };

        // Poll for AI's first reply (e7e5)
        let mut ai_move1 = None;
        for _ in 0..100 {
            ai_move1 = opponent.poll_move(&pos1, empty_sensors());
            if ai_move1.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let ai_move1 = ai_move1.expect("should receive first AI move");
        let pos2 = {
            let mut p = pos1.clone();
            p.play_unchecked(ai_move1);
            p
        };

        // Move 2: human plays d1h5
        let human2_uci = UciMove::from_str("d1h5").unwrap();
        let human2_move = human2_uci.to_move(&pos2).unwrap();
        opponent.opponent_moved(&pos2, &human2_move);

        let pos3 = {
            let mut p = pos2.clone();
            p.play_unchecked(human2_move);
            p
        };

        // Poll for AI's second reply (a7a6) — this should also trigger game over
        let mut ai_move2 = None;
        for _ in 0..100 {
            ai_move2 = opponent.poll_move(&pos3, empty_sensors());
            if ai_move2.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(ai_move2.is_some(), "should receive second AI move");

        // Now poll again — should get GameOver, setting game_over flag
        // The game_over flag is set on the next poll_move call
        for _ in 0..100 {
            let _ = opponent.poll_move(&pos3, empty_sensors());
            if opponent.status() == PlayerStatus::GameOver {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(
            opponent.status() == PlayerStatus::GameOver,
            "game should be over after checkmate"
        );
        assert!(
            opponent.status() != PlayerStatus::Error,
            "game over is not an error"
        );
    }

    // --- JSON parser tests ---

    #[test]
    fn extract_json_string_basic() {
        let json = r#"{"type":"gameFull","id":"abc12345"}"#;
        assert_eq!(extract_json_string(json, "type"), Some("gameFull".into()));
        assert_eq!(extract_json_string(json, "id"), Some("abc12345".into()));
        assert_eq!(extract_json_string(json, "missing"), None);
    }

    #[test]
    fn extract_json_string_with_spaces() {
        let json = r#"{"type" : "gameState" , "status" : "started"}"#;
        assert_eq!(extract_json_string(json, "type"), Some("gameState".into()));
        assert_eq!(extract_json_string(json, "status"), Some("started".into()));
    }

    #[test]
    fn extract_json_string_non_string_value() {
        // Should return None when the value is not a string (e.g. number, bool)
        let json = r#"{"level":4,"rated":true}"#;
        assert_eq!(extract_json_string(json, "level"), None);
        assert_eq!(extract_json_string(json, "rated"), None);
    }

    #[test]
    fn extract_json_object_basic() {
        let json = r#"{"state":{"moves":"e2e4","status":"started"}}"#;
        let obj = extract_json_object(json, "state").unwrap();
        assert_eq!(obj, r#"{"moves":"e2e4","status":"started"}"#);
    }

    #[test]
    fn extract_json_object_nested() {
        let json = r#"{"outer":{"inner":{"deep":"value"},"other":"x"}}"#;
        let obj = extract_json_object(json, "outer").unwrap();
        assert_eq!(obj, r#"{"inner":{"deep":"value"},"other":"x"}"#);
    }

    #[test]
    fn extract_json_object_with_string_braces() {
        // Braces inside strings should not confuse the parser
        let json = r#"{"state":{"moves":"e2e4","note":"use {curly} braces"}}"#;
        let obj = extract_json_object(json, "state").unwrap();
        assert!(obj.contains(r#""moves":"e2e4""#));
        assert!(obj.contains("{curly}"));
    }

    #[test]
    fn extract_json_object_missing() {
        let json = r#"{"type":"gameFull","id":"abc"}"#;
        assert_eq!(extract_json_object(json, "state"), None);
    }

    #[test]
    fn parse_game_event_game_full() {
        let line = r#"{"type":"gameFull","id":"abc12345","initialFen":"startpos","state":{"moves":"","status":"started"}}"#;
        let event = parse_game_event(line).unwrap().unwrap();
        match event {
            GameEvent::GameFull {
                id,
                initial_fen,
                state,
            } => {
                assert_eq!(id, "abc12345");
                assert_eq!(initial_fen, "startpos");
                assert_eq!(state.moves, "");
                assert_eq!(state.status, GameStatus::Started);
                assert_eq!(state.winner, None);
            }
            _ => panic!("expected GameFull"),
        }
    }

    #[test]
    fn parse_game_event_game_state_with_moves() {
        let line = r#"{"type":"gameState","moves":"e2e4 e7e5 d2d4","status":"started"}"#;
        let event = parse_game_event(line).unwrap().unwrap();
        match event {
            GameEvent::GameState(state) => {
                assert_eq!(state.moves, "e2e4 e7e5 d2d4");
                assert_eq!(state.status, GameStatus::Started);
            }
            _ => panic!("expected GameState"),
        }
    }

    #[test]
    fn parse_game_event_checkmate_with_winner() {
        let line = r#"{"type":"gameState","moves":"e2e4 e7e5","status":"mate","winner":"white"}"#;
        let event = parse_game_event(line).unwrap().unwrap();
        match event {
            GameEvent::GameState(state) => {
                assert_eq!(state.status, GameStatus::Mate);
                assert_eq!(state.winner, Some(shakmaty::Color::White));
            }
            _ => panic!("expected GameState"),
        }
    }

    #[test]
    fn parse_game_event_unknown_type_skipped() {
        let line = r#"{"type":"chatLine","username":"bob","text":"hello"}"#;
        assert!(parse_game_event(line).is_none());
    }

    #[test]
    fn parse_game_event_no_type_is_error() {
        let line = r#"{"moves":"e2e4","status":"started"}"#;
        let result = parse_game_event(line).unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn parse_game_event_game_full_missing_state() {
        let line = r#"{"type":"gameFull","id":"abc12345"}"#;
        let result = parse_game_event(line).unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn parse_state_data_all_statuses() {
        for (input, expected) in [
            ("started", GameStatus::Started),
            ("created", GameStatus::Started),
            ("mate", GameStatus::Mate),
            ("resign", GameStatus::Resign),
            ("stalemate", GameStatus::Stalemate),
            ("draw", GameStatus::Draw),
            ("aborted", GameStatus::Aborted),
            ("timeout", GameStatus::Timeout),
            ("outoftime", GameStatus::OutOfTime),
        ] {
            let json = format!(r#"{{"moves":"","status":"{input}"}}"#);
            let state = parse_state_data(&json).unwrap();
            assert_eq!(state.status, expected, "failed for status: {input}");
        }
    }

    #[test]
    fn parse_state_data_unknown_status() {
        let json = r#"{"moves":"","status":"cheat"}"#;
        let state = parse_state_data(json).unwrap();
        assert_eq!(state.status, GameStatus::Other("cheat".into()));
    }

    #[test]
    fn parse_state_data_missing_status_is_error() {
        let json = r#"{"moves":"e2e4"}"#;
        assert!(parse_state_data(json).is_err());
    }

    #[test]
    fn parse_real_lichess_game_full() {
        // Simplified but structurally accurate gameFull event from Lichess
        let line = r#"{"type":"gameFull","id":"gZlNuPte","rated":false,"variant":{"key":"standard"},"clock":{"initial":10800000,"increment":180000},"speed":"classical","perf":{"name":"Classical"},"createdAt":1711305600000,"white":{"id":"player1","name":"Player1","rating":1500},"black":{"aiLevel":4},"initialFen":"startpos","state":{"type":"gameState","moves":"","status":"started","wtime":10800000,"btime":10800000,"winc":180000,"binc":180000}}"#;
        let event = parse_game_event(line).unwrap().unwrap();
        match event {
            GameEvent::GameFull {
                id,
                initial_fen,
                state,
            } => {
                assert_eq!(id, "gZlNuPte");
                assert_eq!(initial_fen, "startpos");
                assert_eq!(state.status, GameStatus::Started);
                assert_eq!(state.moves, "");
            }
            _ => panic!("expected GameFull"),
        }
    }
}
