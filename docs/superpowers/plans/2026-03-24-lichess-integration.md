# Lichess AI Integration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable the smart chess board to play against Lichess Stockfish AI over WiFi using the Lichess Board API, with automatic fallback to the embedded heuristic engine when WiFi or the token is unavailable.

**Architecture:** A platform-independent `src/lichess.rs` module defines traits (`LichessClient`, `LichessGame`, `LichessStream`) and types (`GameEvent`, `LichessConfig`, `LichessMessage`). A `LichessOpponent` struct implements the existing `Opponent` trait using background-thread channels. ESP32-specific HTTP implementation lives in `src/esp32/lichess.rs`; a host stub lives in `src/mock/lichess.rs`. The `Opponent` trait gains a `position` parameter on `poll_move` and a `has_error()` method. `GameSession` is updated to poll every tick and merge failure status.

**Tech Stack:** Rust (edition 2024), shakmaty 0.29.4, esp-idf-svc 0.52.1 (ESP-IDF v5.5.3), std::sync::mpsc channels, no new crate dependencies.

**VCS:** This project uses jj (Jujutsu). Every task begins with a "Start change" step that ensures `@` is empty before describing the new work:

```bash
# If @ has changes, start a new change; then describe what we're about to do
jj log -r @ --no-graph -T 'if(empty, "empty", "not-empty")' | grep -q "not-empty" && jj new
jj desc -m "<message>"
```

There is no separate "finalize" step at the end of each task — the next task's start step handles it. After the final task, run `jj new` once to close it out.

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `src/lichess.rs` | Platform-independent traits (`LichessClient`, `LichessGame`, `LichessStream`), types (`GameEvent`, `GameStateData`, `GameStatus`, `LichessConfig`, `LichessMessage`, `SpawnError`), `LichessOpponent` struct, `spawn_lichess_opponent()` function |
| Create | `src/esp32/lichess.rs` | `Esp32LichessClient`, `Esp32LichessGame`, `Esp32LichessStream` — HTTP implementations using `esp-idf-svc::http::client` |
| Create | `src/mock/lichess.rs` | `MockLichessClient`, `MockLichessGame`, `MockLichessStream` — stub implementations that return errors (real host networking deferred) |
| Modify | `src/opponent.rs` | Add `position: &Chess` param to `poll_move`, add `has_error()` default method |
| Modify | `src/session.rs` | Split `handle_opponent` into per-human-move and per-tick polling; merge `StatusKind::Failure` on error |
| Modify | `src/main.rs` | ESP32: construct `LichessConfig`, create client, call `spawn_lichess_opponent`, fallback to `EmbeddedEngine`. Host: no change. |
| Modify | `src/lib.rs` | Add `pub mod lichess` |
| Modify | `src/esp32/mod.rs` | Add `pub mod lichess` and re-export |
| Modify | `src/mock/mod.rs` | Add `pub mod lichess` and re-export |
| Modify | `build.rs` | Add compile-time env var parsing/validation for `LICHESS_AI_LEVEL`, `LICHESS_CLOCK_LIMIT`, `LICHESS_CLOCK_INCREMENT` |
| Modify | `.env.example` | Add Lichess env var documentation |
| Modify | `tests/feedback_integration.rs` | Add test for `StatusKind::Failure` feedback when opponent has error |

---

### Task 0: Validate Lichess API token scopes (manual, human-only)

**Files:** None — this is a manual API validation step.

Per the spec, this must happen before writing any code. The Lichess OpenAPI spec lists `challenge:write`, `bot:play`, and `board:play` as the security requirement for `POST /api/challenge/ai`, but `bot:play` may not actually be needed (and requires an irreversible Bot account upgrade).

- [ ] **Step 1: Generate a personal access token**

Go to https://lichess.org/account/oauth/token and create a token with scopes: `board:play`, `challenge:write`.

- [ ] **Step 2: Test the token with a curl request**

```bash
curl -v -X POST https://lichess.org/api/challenge/ai \
  -H "Authorization: Bearer <token>" \
  -d "level=1&color=white&variant=standard&clock.limit=60&clock.increment=0"
```

Expected: HTTP 201 with a JSON body containing an `id` field.

- [ ] **Step 3: If 401/403, add `bot:play` scope and retry**

If the request fails with 401 or 403, regenerate the token with `board:play`, `challenge:write`, `bot:play`. If `bot:play` requires a Bot account upgrade (irreversible), stop and make a product decision before proceeding.

- [ ] **Step 4: Record the working scopes in `.env`**

Add the validated token to `.env`:

```
LICHESS_API_TOKEN=lip_...
```

**This task blocks all other tasks.** Do not proceed until the token is validated.

---

### Task 1: Modify `Opponent` trait and update `EmbeddedEngine`

**Files:**
- Modify: `src/opponent.rs`

The `Opponent` trait needs two changes per the spec: `poll_move` gains a `position: &Chess` parameter (so `LichessOpponent` can parse UCI into a `shakmaty::Move` without storing stale state), and a new `has_error()` method with a default `false` implementation (so `EmbeddedEngine` needs no changes for it).

- [ ] **Step 1: Start change**

```bash
jj log -r @ --no-graph -T 'if(empty, "empty", "not-empty")' | grep -q "not-empty" && jj new
jj desc -m "refactor: add position param to Opponent::poll_move and has_error method"
```

- [ ] **Step 2: Write failing test for new `poll_move` signature**

Add a test in `src/opponent.rs` `mod tests` that calls `poll_move` with a position argument:

```rust
#[test]
fn poll_move_accepts_position() {
    let pos = position_from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
    let mut engine = EmbeddedEngine::new(42);
    engine.start_thinking(&pos, &dummy_move());
    let _mv = engine.poll_move(&pos);
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `just test -- opponent::tests::poll_move_accepts_position -v`
Expected: FAIL — `poll_move` doesn't accept a `&Chess` argument yet.

- [ ] **Step 4: Update trait and `EmbeddedEngine` implementation**

In `src/opponent.rs`, change the trait:

```rust
pub trait Opponent {
    fn start_thinking(&mut self, position: &Chess, human_move: &Move);
    fn poll_move(&mut self, position: &Chess) -> Option<Move>;  // added position param
    fn has_error(&self) -> bool { false }                        // new, default no-op
}
```

Update `EmbeddedEngine::poll_move`:

```rust
fn poll_move(&mut self, _position: &Chess) -> Option<Move> {
    self.pending.take()
}
```

- [ ] **Step 5: Fix all call sites**

Update `src/session.rs` `handle_opponent` — change `opponent.poll_move()` to `opponent.poll_move(self.engine.position())`.

Update `tests/feedback_integration.rs` — any direct `poll_move()` calls need the position argument. (The integration tests use `GameSession` which calls `poll_move` internally, so only `session.rs` needs updating.)

- [ ] **Step 6: Run all tests to verify everything passes**

Run: `just test`
Expected: All tests pass.

---

### Task 2: Update `GameSession` for async opponent polling and error feedback

**Files:**
- Modify: `src/session.rs`
- Modify: `src/feedback.rs` (add `with_merged_status` method)

Currently `handle_opponent` calls `start_thinking` and `poll_move` in the same tick, gated on `state.human_move()`. For Lichess, the AI reply arrives asynchronously — potentially many ticks later. We need to:
1. Call `start_thinking` only when a human move is detected.
2. Call `poll_move` every tick (regardless of human move).
3. Check `has_error()` every tick and merge `StatusKind::Failure` into feedback.

- [ ] **Step 1: Start change**

```bash
jj log -r @ --no-graph -T 'if(empty, "empty", "not-empty")' | grep -q "not-empty" && jj new
jj desc -m "feat: split opponent handling for async polling and add error feedback"
```

- [ ] **Step 2: Write failing test for async polling**

Add a test with a `DelayedOpponent` that returns `None` from `poll_move` for N calls, then returns a move:

```rust
#[cfg(test)]
struct DelayedOpponent {
    delay_ticks: usize,
    ticks_remaining: usize,
    pending: Option<Move>,
}

#[cfg(test)]
impl DelayedOpponent {
    fn new(delay_ticks: usize) -> Self {
        Self {
            delay_ticks,
            ticks_remaining: 0,
            pending: None,
        }
    }
}

#[cfg(test)]
impl Opponent for DelayedOpponent {
    fn start_thinking(&mut self, position: &Chess, _human_move: &Move) {
        // Pick first legal move
        let moves = position.legal_moves();
        self.pending = moves.into_iter().next();
        self.ticks_remaining = self.delay_ticks;
    }

    fn poll_move(&mut self, _position: &Chess) -> Option<Move> {
        if self.pending.is_some() && self.ticks_remaining == 0 {
            self.pending.take()
        } else {
            if self.ticks_remaining > 0 {
                self.ticks_remaining -= 1;
            }
            None
        }
    }
}

#[test]
fn async_opponent_returns_move_after_delay() {
    let mut sensor = ScriptedSensor::new();
    let mut session = GameSession::with_opponent(Box::new(DelayedOpponent::new(2)));

    // Human plays e2-e4
    sensor.push_script("e2 We4.").unwrap();
    let result = run_script(&mut sensor, &mut session);

    // Move detected but opponent hasn't replied yet (delay=2)
    assert!(result.state.human_move().is_some());
    assert!(result.computer_move.is_none());

    // Tick 1: still waiting
    let positions = sensor.read_positions();
    let result = session.process_positions(positions);
    assert!(result.computer_move.is_none());

    // Tick 2: still waiting (ticks_remaining was 2, now 1)
    let positions = sensor.read_positions();
    let result = session.process_positions(positions);
    assert!(result.computer_move.is_none());

    // Tick 3: move arrives
    let positions = sensor.read_positions();
    let result = session.process_positions(positions);
    assert!(result.computer_move.is_some());
    assert_eq!(session.position().turn(), Color::White);
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `just test -- session::tests::async_opponent_returns_move_after_delay -v`
Expected: FAIL — current `handle_opponent` only polls when `human_move` is `Some`.

- [ ] **Step 4: Write failing test for error feedback**

```rust
#[cfg(test)]
struct ErrorOpponent {
    errored: bool,
}

#[cfg(test)]
impl Opponent for ErrorOpponent {
    fn start_thinking(&mut self, _position: &Chess, _human_move: &Move) {
        self.errored = true;
    }
    fn poll_move(&mut self, _position: &Chess) -> Option<Move> {
        None
    }
    fn has_error(&self) -> bool {
        self.errored
    }
}

#[test]
fn error_opponent_produces_failure_feedback() {
    use crate::feedback::StatusKind;

    let mut sensor = ScriptedSensor::new();
    let mut session =
        GameSession::with_opponent(Box::new(ErrorOpponent { errored: false }));

    // Human plays — triggers start_thinking which sets error
    sensor.push_script("e2 We4.").unwrap();
    let result = run_script(&mut sensor, &mut session);

    assert_eq!(result.feedback.status(), Some(StatusKind::Failure));
}
```

- [ ] **Step 5: Run test to verify it fails**

Run: `just test -- session::tests::error_opponent_produces_failure_feedback -v`
Expected: FAIL — `handle_opponent` doesn't check `has_error()`.

- [ ] **Step 6: Refactor `handle_opponent` and `process_positions`**

Split `handle_opponent` into two methods and add error checking:

```rust
fn handle_human_move(&mut self, state: &GameState) {
    let Some(opponent) = self.opponent.as_mut() else { return };
    let Some(human_move) = state.human_move() else { return };
    opponent.start_thinking(self.engine.position(), human_move);
}

fn poll_opponent_move(&mut self) -> Option<Move> {
    let opponent = self.opponent.as_mut()?;
    let reply = opponent.poll_move(self.engine.position())?;
    match self.engine.apply_opponent_move(&reply) {
        Ok(()) => Some(reply),
        Err(e) => {
            log::warn!("Computer move failed: {e}");
            None
        }
    }
}
```

Update `process_positions`:

```rust
pub fn process_positions(&mut self, positions: ByColor<Bitboard>) -> TickResult {
    let state = self.engine.tick(positions);

    self.handle_human_move(&state);
    let computer_move = self.poll_opponent_move();

    let mut feedback = compute_feedback(&state);

    // Merge error status if opponent has failed
    if self.opponent.as_ref().is_some_and(|o| o.has_error()) {
        feedback = feedback.with_merged_status(StatusKind::Failure);
    }

    let feedback = if feedback.is_empty() {
        recovery_feedback(&self.engine.expected_positions(), &positions)
            .unwrap_or(feedback)
    } else {
        feedback
    };

    TickResult {
        state,
        feedback,
        computer_move,
    }
}
```

This requires adding a `with_merged_status` method to `BoardFeedback` in `src/feedback.rs`:

```rust
/// Return a copy with the given status merged in (overwrites any existing status).
pub fn with_merged_status(mut self, kind: StatusKind) -> Self {
    self.status = Some(kind);
    self
}
```

- [ ] **Step 7: Run all tests to verify everything passes**

Run: `just test`
Expected: All tests pass, including the two new ones.

---

### Task 3: Create `src/lichess.rs` — types and traits

**Files:**
- Create: `src/lichess.rs`
- Modify: `src/lib.rs`

This task creates the platform-independent types and trait definitions. No implementations yet — just the public API surface that ESP32 and mock modules will implement.

- [ ] **Step 1: Start change**

```bash
jj log -r @ --no-graph -T 'if(empty, "empty", "not-empty")' | grep -q "not-empty" && jj new
jj desc -m "feat: add lichess module with platform-independent types and traits"
```

- [ ] **Step 2: Create `src/lichess.rs` with all types**

```rust
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
```

- [ ] **Step 3: Add traits to `src/lichess.rs`**

```rust
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
```

- [ ] **Step 4: Add `pub mod lichess` to `src/lib.rs`**

Add `pub mod lichess;` to `src/lib.rs`, unconditionally (the module is platform-independent):

```rust
pub mod lichess;
```

- [ ] **Step 5: Run tests to verify compilation**

Run: `just test`
Expected: All tests pass. The new module compiles but has no tests yet.

---

### Task 4: Implement `LichessOpponent` and `spawn_lichess_opponent`

**Files:**
- Modify: `src/lichess.rs`

This is the core orchestration logic — platform-independent, testable with mock implementations. `LichessOpponent` implements `Opponent` using channels to a background worker. `spawn_lichess_opponent` creates the game, spawns the worker, and waits for startup confirmation.

- [ ] **Step 1: Start change**

```bash
jj log -r @ --no-graph -T 'if(empty, "empty", "not-empty")' | grep -q "not-empty" && jj new
jj desc -m "feat: implement LichessOpponent and spawn_lichess_opponent with worker loop"
```

- [ ] **Step 2: Write test infrastructure — mock client for unit testing**

Add to `src/lichess.rs` inside a `#[cfg(test)] mod tests` block:

```rust
#[cfg(all(test, not(target_os = "espidf")))]
mod tests {
    use super::*;
    use shakmaty::{Chess, Position, uci::UciMove};
    use std::str::FromStr;

    /// A mock LichessStream that replays a scripted sequence of events.
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
            self.game.ok_or_else(|| "mock: no game configured".to_string())
        }
    }

    fn spawn_thread(f: Box<dyn FnOnce() + Send>) -> Result<(), String> {
        std::thread::spawn(f);
        Ok(())
    }
}
```

- [ ] **Step 3: Write failing test for basic spawn + AI move flow**

```rust
#[test]
fn spawn_and_receive_ai_move() {
    let game = MockGame {
        game_id: "test1234".to_string(),
        events: vec![
            Ok(GameEvent::GameFull {
                id: "test1234".to_string(),
                initial_fen: "startpos".to_string(),
                state: GameStateData {
                    moves: String::new(),
                    status: GameStatus::Started,
                    winner: None,
                },
            }),
            // After player sends e2e4, AI replies with e7e5
            Ok(GameEvent::GameState(GameStateData {
                moves: "e2e4 e7e5".to_string(),
                status: GameStatus::Started,
                winner: None,
            })),
        ],
    };

    let client = MockClient { game: Some(game) };
    let config = LichessConfig {
        level: 1,
        clock_limit: 600,
        clock_increment: 0,
    };

    let mut opponent = spawn_lichess_opponent(client, config, spawn_thread)
        .expect("spawn should succeed");

    // Simulate human playing e2e4
    let pos = Chess::default();
    let human_move = UciMove::from_str("e2e4").unwrap()
        .to_move(&pos).unwrap();
    opponent.start_thinking(&pos, &human_move);

    // Poll until we get the AI move
    let new_pos = pos.play(&human_move).unwrap();
    let mut ai_move = None;
    for _ in 0..100 {
        ai_move = opponent.poll_move(&new_pos);
        if ai_move.is_some() { break; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert!(ai_move.is_some(), "should receive AI move");
    assert!(!opponent.has_error());
}
```

- [ ] **Step 4: Run test to verify it fails**

Run: `just test -- lichess::tests::spawn_and_receive_ai_move -v`
Expected: FAIL — `spawn_lichess_opponent` and `LichessOpponent` don't exist yet.

- [ ] **Step 5: Implement `LichessOpponent`**

Add to `src/lichess.rs`:

```rust
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use shakmaty::{Chess, Move, Position, uci::UciMove};
use std::str::FromStr;
use crate::opponent::Opponent;

pub struct LichessOpponent {
    player_move_tx: SyncSender<String>,
    ai_move_rx: Receiver<LichessMessage>,
    error: bool,
    game_over: bool,
}

impl Opponent for LichessOpponent {
    fn start_thinking(&mut self, _position: &Chess, human_move: &Move) {
        if self.error || self.game_over { return; }
        let uci = UciMove::from_standard(human_move).to_string();
        if self.player_move_tx.send(uci).is_err() {
            log::warn!("Lichess worker disconnected");
            self.error = true;
        }
    }

    fn poll_move(&mut self, position: &Chess) -> Option<Move> {
        if self.error || self.game_over { return None; }
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
                    None // expected — worker exits after game over
                } else {
                    log::warn!("Lichess worker disconnected unexpectedly");
                    self.error = true;
                    None
                }
            }
        }
    }

    fn has_error(&self) -> bool {
        self.error
    }
}
```

- [ ] **Step 6: Implement `spawn_lichess_opponent`**

```rust
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

    // Wait for worker to signal ready or error
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(LichessOpponent {
            player_move_tx: player_tx,
            ai_move_rx: ai_rx,
            error: false,
            game_over: false,
        }),
        Ok(Err(desc)) => Err(SpawnError::WorkerStartup(desc)),
        Err(_) => Err(SpawnError::WorkerStartup(
            "worker exited before signalling ready".into(),
        )),
    }
}
```

- [ ] **Step 7: Implement `worker_loop`**

```rust
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

    // Read GameFull event
    let initial_move_count = match stream.next_event() {
        Some(Ok(GameEvent::GameFull { state, .. })) => {
            state.moves.split_whitespace().count()
        }
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

    // Signal ready
    let _ = ready_tx.send(Ok(()));

    let mut expected_move_count = initial_move_count;

    loop {
        // Wait for player move
        let player_uci = match player_rx.recv() {
            Ok(uci) => uci,
            Err(_) => return, // main thread dropped — game over
        };

        if let Err(e) = stream.make_move(&player_uci) {
            let _ = ai_tx.send(LichessMessage::Error(format!("make_move failed: {e}")));
            return;
        }

        expected_move_count += 2; // player + AI

        // Read events until we get the AI's reply or game ends
        loop {
            match stream.next_event() {
                Some(Ok(GameEvent::GameState(state))) | Some(Ok(GameEvent::GameFull { state, .. })) => {
                    let move_count = state.moves.split_whitespace().count();

                    if move_count >= expected_move_count {
                        // AI has replied — extract last move
                        let ai_uci = state.moves.split_whitespace().last()
                            .unwrap_or_default().to_string();
                        let _ = ai_tx.send(LichessMessage::AiMove(ai_uci));

                        if state.status.is_terminal() {
                            let _ = ai_tx.send(LichessMessage::GameOver);
                            return;
                        }
                        break; // back to waiting for next player move
                    } else if state.status.is_terminal() {
                        // Human's move ended the game (no AI reply)
                        let _ = ai_tx.send(LichessMessage::GameOver);
                        return;
                    }
                    // else: intermediate event, keep reading
                }
                Some(Err(e)) => {
                    let _ = ai_tx.send(LichessMessage::Error(format!("stream error: {e}")));
                    return;
                }
                None => {
                    let _ = ai_tx.send(LichessMessage::Error(
                        "stream closed unexpectedly".into(),
                    ));
                    return;
                }
            }
        }
    }
}
```

- [ ] **Step 8: Run all tests**

Run: `just test`
Expected: All tests pass, including `spawn_and_receive_ai_move`.

- [ ] **Step 9: Write additional tests**

Add tests for error cases:

```rust
#[test]
fn spawn_fails_when_challenge_fails() {
    let client = MockClient { game: None };
    let config = LichessConfig {
        level: 1, clock_limit: 600, clock_increment: 0,
    };
    let result = spawn_lichess_opponent(client, config, spawn_thread);
    assert!(matches!(result, Err(SpawnError::Http(_))));
}

#[test]
fn worker_signals_error_on_stream_close() {
    let game = MockGame {
        game_id: "test1234".to_string(),
        events: vec![
            Ok(GameEvent::GameFull {
                id: "test1234".to_string(),
                initial_fen: "startpos".to_string(),
                state: GameStateData {
                    moves: String::new(),
                    status: GameStatus::Started,
                    winner: None,
                },
            }),
            // Stream closes after GameFull — no GameState will arrive
        ],
    };

    let client = MockClient { game: Some(game) };
    let config = LichessConfig {
        level: 1, clock_limit: 600, clock_increment: 0,
    };

    let mut opponent = spawn_lichess_opponent(client, config, spawn_thread)
        .expect("spawn should succeed");

    let pos = Chess::default();
    let human_move = UciMove::from_str("e2e4").unwrap()
        .to_move(&pos).unwrap();
    opponent.start_thinking(&pos, &human_move);

    // Poll until error is detected
    let new_pos = pos.play(&human_move).unwrap();
    for _ in 0..100 {
        let _ = opponent.poll_move(&new_pos);
        if opponent.has_error() { break; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert!(opponent.has_error());
}

#[test]
fn game_over_after_checkmate() {
    let game = MockGame {
        game_id: "test1234".to_string(),
        events: vec![
            Ok(GameEvent::GameFull {
                id: "test1234".to_string(),
                initial_fen: "startpos".to_string(),
                state: GameStateData {
                    moves: String::new(),
                    status: GameStatus::Started,
                    winner: None,
                },
            }),
            Ok(GameEvent::GameState(GameStateData {
                moves: "e2e4 e7e5".to_string(),
                status: GameStatus::Mate,
                winner: Some(shakmaty::Color::White),
            })),
        ],
    };

    let client = MockClient { game: Some(game) };
    let config = LichessConfig {
        level: 1, clock_limit: 600, clock_increment: 0,
    };

    let mut opponent = spawn_lichess_opponent(client, config, spawn_thread)
        .expect("spawn should succeed");

    let pos = Chess::default();
    let human_move = UciMove::from_str("e2e4").unwrap()
        .to_move(&pos).unwrap();
    opponent.start_thinking(&pos, &human_move);

    let new_pos = pos.play(&human_move).unwrap();
    let mut got_move = false;
    for _ in 0..100 {
        if opponent.poll_move(&new_pos).is_some() {
            got_move = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert!(got_move, "should receive AI move even when game ends");
    assert!(!opponent.has_error(), "game over is not an error");
}
```

- [ ] **Step 10: Run all tests**

Run: `just test`
Expected: All tests pass.

---

### Task 5: Create mock Lichess stubs for host builds

**Files:**
- Create: `src/mock/lichess.rs`
- Modify: `src/mock/mod.rs`

The host build needs stub implementations so the code compiles on non-ESP targets. Per the spec, real host networking is deferred — these stubs return errors immediately.

- [ ] **Step 1: Start change**

```bash
jj log -r @ --no-graph -T 'if(empty, "empty", "not-empty")' | grep -q "not-empty" && jj new
jj desc -m "feat: add mock Lichess stubs for host builds"
```

- [ ] **Step 2: Create `src/mock/lichess.rs`**

```rust
use crate::lichess::{GameEvent, LichessClient, LichessGame, LichessStream};

#[derive(Debug)]
pub struct MockLichessClient;

pub struct MockLichessGame;

pub struct MockLichessStream;

impl LichessClient for MockLichessClient {
    type Error = String;
    type Game = MockLichessGame;

    fn challenge_ai(
        self,
        _level: u8,
        _clock_limit: u32,
        _clock_increment: u32,
    ) -> Result<MockLichessGame, String> {
        Err("Lichess HTTP not implemented on host".to_string())
    }
}

impl LichessGame for MockLichessGame {
    type Error = String;

    fn game_id(&self) -> &str {
        "mock"
    }

    fn into_stream(self) -> Result<Box<dyn LichessStream<Error = String>>, String> {
        Err("Lichess HTTP not implemented on host".to_string())
    }
}

impl LichessStream for MockLichessStream {
    type Error = String;

    fn next_event(&mut self) -> Option<Result<GameEvent, String>> {
        None
    }

    fn make_move(&mut self, _uci_move: &str) -> Result<(), String> {
        Err("Lichess HTTP not implemented on host".to_string())
    }
}
```

- [ ] **Step 3: Add `pub mod lichess` to `src/mock/mod.rs`**

```rust
pub mod lichess;
```

And add the re-export:

```rust
pub use lichess::MockLichessClient;
```

- [ ] **Step 4: Run tests to verify compilation**

Run: `just test`
Expected: All tests pass.

---

### Task 6: Create ESP32 Lichess HTTP implementation

**Files:**
- Create: `src/esp32/lichess.rs`
- Modify: `src/esp32/mod.rs`

This is the real HTTP implementation using `esp-idf-svc::http::client`. It implements `LichessClient`, `LichessGame`, and `LichessStream` for the ESP32 target. Key constraints: `EspHttpConnection` is `!Send`, so all HTTP connections are created on the thread that uses them.

- [ ] **Step 1: Start change**

```bash
jj log -r @ --no-graph -T 'if(empty, "empty", "not-empty")' | grep -q "not-empty" && jj new
jj desc -m "feat: add ESP32 Lichess HTTP implementation"
```

- [ ] **Step 2: Create `src/esp32/lichess.rs` with `Esp32LichessClient`**

The `esp-idf-svc` v0.52.1 HTTP client uses `Client::wrap(EspHttpConnection::new(&config))`. The `Client` owns the connection. `client.request()` returns `Request<&mut EspHttpConnection>` which borrows the client. `request.submit()` returns `Response<&mut EspHttpConnection>` which also borrows. The response must be fully consumed before the client can be reused. Reads use `embedded_io::Read`, not `std::io::Read`.

```rust
use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::http::Method;
use embedded_svc::io::Write;
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use esp_idf_svc::sys::esp_crt_bundle_attach;
use crate::lichess::{
    GameEvent, GameStateData, GameStatus, LichessClient, LichessGame, LichessStream,
};
use shakmaty::Color;

const LICHESS_BASE: &str = "https://lichess.org";

#[derive(Debug)]
pub enum Esp32LichessError {
    Http(String),
    Parse(String),
    Io(String),
}

impl std::fmt::Display for Esp32LichessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "HTTP error: {e}"),
            Self::Parse(e) => write!(f, "parse error: {e}"),
            Self::Io(e) => write!(f, "IO error: {e}"),
        }
    }
}

pub struct Esp32LichessClient {
    token: &'static str,
}

impl Esp32LichessClient {
    pub fn new(token: &'static str) -> Self {
        Self { token }
    }
}

pub struct Esp32LichessGame {
    game_id: String,
    token: &'static str,
}

impl LichessClient for Esp32LichessClient {
    type Error = Esp32LichessError;
    type Game = Esp32LichessGame;

    fn challenge_ai(
        self,
        level: u8,
        clock_limit: u32,
        clock_increment: u32,
    ) -> Result<Esp32LichessGame, Esp32LichessError> {
        let config = HttpConfig {
            timeout: Some(std::time::Duration::from_secs(10)),
            crt_bundle_attach: Some(esp_crt_bundle_attach),
            ..Default::default()
        };

        let mut client = HttpClient::wrap(
            EspHttpConnection::new(&config)
                .map_err(|e| Esp32LichessError::Http(e.to_string()))?,
        );

        let url = format!("{LICHESS_BASE}/api/challenge/ai");
        let body = format!(
            "level={level}&color=white&variant=standard\
             &clock.limit={clock_limit}&clock.increment={clock_increment}"
        );
        let content_len = format!("{}", body.len());
        let auth = format!("Bearer {}", self.token);
        let headers = [
            ("Authorization", auth.as_str()),
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Content-Length", content_len.as_str()),
        ];

        let mut request = client
            .request(Method::Post, &url, &headers)
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        request
            .write_all(body.as_bytes())
            .map_err(|e| Esp32LichessError::Io(e.to_string()))?;

        request
            .flush()
            .map_err(|e| Esp32LichessError::Io(e.to_string()))?;

        let mut response = request
            .submit()
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        let status = response.status();
        if status != 201 {
            let mut buf = [0u8; 512];
            let n = response.read(&mut buf).unwrap_or(0);
            let body_str = core::str::from_utf8(&buf[..n]).unwrap_or("<non-utf8>");
            return Err(Esp32LichessError::Http(
                format!("challenge_ai returned {status}: {body_str}")
            ));
        }

        // Read response body and parse game ID
        let mut buf = [0u8; 1024];
        let n = embedded_svc::utils::io::try_read_full(&mut response, &mut buf)
            .map_err(|e| Esp32LichessError::Io(e.0.to_string()))?;
        let body_str = core::str::from_utf8(&buf[..n])
            .map_err(|e| Esp32LichessError::Parse(e.to_string()))?;

        let game_id = extract_json_string(body_str, "id")
            .ok_or_else(|| Esp32LichessError::Parse(
                format!("no 'id' in response: {body_str}")
            ))?;

        log::info!("Lichess challenge created: game_id={game_id}");

        Ok(Esp32LichessGame {
            game_id,
            token: self.token,
        })
    }
}

impl LichessGame for Esp32LichessGame {
    type Error = Esp32LichessError;

    fn game_id(&self) -> &str {
        &self.game_id
    }

    fn into_stream(
        self,
    ) -> Result<Box<dyn LichessStream<Error = Esp32LichessError>>, Esp32LichessError> {
        Esp32LichessStreamImpl::connect(self.token, &self.game_id)
            .map(|s| Box::new(s) as Box<dyn LichessStream<Error = Esp32LichessError>>)
    }
}
```

- [ ] **Step 3: Implement `Esp32LichessStreamImpl` struct and `connect`**

Per the spec, `into_stream()` creates both the long-lived NDJSON stream connection and a reusable POST client on the worker thread.

**Ownership model:** In `esp-idf-svc` v0.52.1, `Response` borrows `&mut EspHttpConnection` — the response cannot be stored separately from the connection. For the long-lived NDJSON stream, we use the raw `EspHttpConnection` directly (calling `initiate_request` / `initiate_response` / `read` on it) rather than the `Client` wrapper, because we need to hold the connection in response-reading state indefinitely. For POST requests, we use a separate `Client`-wrapped connection that can be reused across moves.

```rust
struct Esp32LichessStreamImpl {
    /// The NDJSON stream connection — held in Response state.
    /// We read bytes directly from this via embedded_io::Read.
    stream_conn: EspHttpConnection,
    /// Reusable POST client for make_move requests.
    post_client: HttpClient<EspHttpConnection>,
    /// Token for POST Authorization header.
    post_token: &'static str,
    /// Game ID for constructing POST URLs.
    game_id: String,
    /// Line buffer for NDJSON parsing — reused across reads.
    line_buf: Vec<u8>,
}

impl Esp32LichessStreamImpl {
    fn connect(token: &'static str, game_id: &str) -> Result<Self, Esp32LichessError> {
        // Stream connection: The spec calls for 10s on initial connection and
        // 60s for steady-state reads, but esp-idf-svc sets timeout per-connection,
        // not per-operation. We use 60s because the steady-state read timeout is
        // the critical one (AI thinking time). The initial HTTP handshake completes
        // in <1s on a working network; a 60s timeout just means slower failure
        // detection on a bad connection at startup, which is acceptable.
        let stream_config = HttpConfig {
            timeout: Some(std::time::Duration::from_secs(60)),
            crt_bundle_attach: Some(esp_crt_bundle_attach),
            ..Default::default()
        };

        let mut stream_conn = EspHttpConnection::new(&stream_config)
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        // Open the NDJSON stream using raw connection methods
        let url = format!("{LICHESS_BASE}/api/board/game/stream/{game_id}");
        let auth_header = format!("Bearer {token}");
        let headers = [("Authorization", auth_header.as_str())];

        stream_conn
            .initiate_request(Method::Get, &url, &headers)
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        stream_conn
            .initiate_response()
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        let status = stream_conn.status();
        if status != 200 {
            return Err(Esp32LichessError::Http(
                format!("stream GET returned {status}")
            ));
        }

        // POST client: 10s timeout, reused across moves
        let post_config = HttpConfig {
            timeout: Some(std::time::Duration::from_secs(10)),
            crt_bundle_attach: Some(esp_crt_bundle_attach),
            ..Default::default()
        };

        let post_client = HttpClient::wrap(
            EspHttpConnection::new(&post_config)
                .map_err(|e| Esp32LichessError::Http(e.to_string()))?,
        );

        Ok(Self {
            stream_conn,
            post_client,
            post_token: token,
            game_id: game_id.to_string(),
            line_buf: Vec::with_capacity(4096),
        })
    }
}
```

- [ ] **Step 4: Implement `read_line` helper**

Reads one `\n`-terminated line from the stream using `embedded_io::Read` (which `EspHttpConnection` implements when in Response state). Returns `Ok(Some(line))` for a complete line, `Ok(None)` if the stream is closed (0 bytes read), or `Err` on I/O failure.

```rust
use embedded_svc::io::Read as _;

impl Esp32LichessStreamImpl {
    fn read_line(&mut self) -> Result<Option<String>, Esp32LichessError> {
        self.line_buf.clear();
        let mut byte = [0u8; 1];
        loop {
            match self.stream_conn.read(&mut byte) {
                Ok(0) => {
                    // Stream closed
                    if self.line_buf.is_empty() {
                        return Ok(None);
                    }
                    let line = String::from_utf8_lossy(&self.line_buf).into_owned();
                    return Ok(Some(line));
                }
                Ok(_) => {
                    if byte[0] == b'\n' {
                        let line = String::from_utf8_lossy(&self.line_buf).into_owned();
                        return Ok(Some(line));
                    }
                    self.line_buf.push(byte[0]);
                }
                Err(e) => {
                    return Err(Esp32LichessError::Io(e.to_string()));
                }
            }
        }
    }
}
```

- [ ] **Step 5: Implement JSON parsing helpers**

```rust
/// Minimal JSON string extractor — avoids pulling in a JSON crate.
/// Finds `"key":"value"` or `"key": "value"` and returns value.
fn extract_json_string(json: &str, key: &str) -> Option<String> {
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
/// Given `"state":{"moves":"e2e4","status":"started"}`, returns
/// `{"moves":"e2e4","status":"started"}`.
fn extract_json_object(json: &str, key: &str) -> Option<String> {
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

fn parse_game_event(line: &str) -> Option<Result<GameEvent, Esp32LichessError>> {
    let event_type = match extract_json_string(line, "type") {
        Some(t) => t,
        None => {
            return Some(Err(Esp32LichessError::Parse(
                format!("no 'type' in event: {line}")
            )));
        }
    };

    match event_type.as_str() {
        "gameFull" => {
            let id = extract_json_string(line, "id").unwrap_or_default();
            let initial_fen = extract_json_string(line, "initialFen")
                .unwrap_or_else(|| "startpos".to_string());
            let state_json = match extract_json_object(line, "state") {
                Some(s) => s,
                None => {
                    return Some(Err(Esp32LichessError::Parse(
                        "no 'state' object in gameFull".into()
                    )));
                }
            };
            let state = match parse_state_data(&state_json) {
                Ok(s) => s,
                Err(e) => return Some(Err(e)),
            };
            Some(Ok(GameEvent::GameFull { id, initial_fen, state }))
        }
        "gameState" => {
            Some(parse_state_data(line).map(GameEvent::GameState))
        }
        // chatLine, opponentGone, etc. — silently skip
        _ => {
            log::debug!("Ignoring Lichess event type: {event_type}");
            None
        }
    }
}

fn parse_state_data(json: &str) -> Result<GameStateData, Esp32LichessError> {
    let moves = extract_json_string(json, "moves").unwrap_or_default();
    let status_str = extract_json_string(json, "status")
        .ok_or_else(|| Esp32LichessError::Parse("no 'status' field".into()))?;
    let winner = extract_json_string(json, "winner").and_then(|w| match w.as_str() {
        "white" => Some(Color::White),
        "black" => Some(Color::Black),
        _ => None,
    });

    Ok(GameStateData {
        moves,
        status: GameStatus::from_str(&status_str),
        winner,
    })
}
```

Note: `parse_game_event` returns `Option<Result<...>>` — `None` means "skip this event" (unknown type), `Some(Ok(...))` is a parsed event, `Some(Err(...))` is a parse failure. This cleanly separates "silently ignored" from "broken."

- [ ] **Step 6: Implement `LichessStream` for `Esp32LichessStreamImpl`**

The `post_client` is reused across moves — after a `Response` is dropped, the underlying `EspHttpConnection` returns to a state where `initiate_request` can be called again. The `Client` wrapper handles this automatically.

```rust
impl LichessStream for Esp32LichessStreamImpl {
    type Error = Esp32LichessError;

    fn next_event(&mut self) -> Option<Result<GameEvent, Esp32LichessError>> {
        loop {
            match self.read_line() {
                Ok(Some(line)) if line.trim().is_empty() => continue, // keep-alive ping
                Ok(Some(line)) => {
                    match parse_game_event(&line) {
                        Some(result) => return Some(result),
                        None => continue, // unknown event type, skip
                    }
                }
                Ok(None) => return None, // stream closed
                Err(e) => return Some(Err(e)),
            }
        }
    }

    fn make_move(&mut self, uci_move: &str) -> Result<(), Esp32LichessError> {
        let url = format!(
            "{LICHESS_BASE}/api/board/game/{}/move/{uci_move}",
            self.game_id
        );
        let auth_header = format!("Bearer {}", self.post_token);
        let headers = [("Authorization", auth_header.as_str())];

        let request = self.post_client
            .request(Method::Post, &url, &headers)
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        let response = request
            .submit()
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        let status = response.status();
        if status != 200 {
            return Err(Esp32LichessError::Http(
                format!("make_move returned {status}")
            ));
        }

        // Response is dropped here, freeing the borrow on post_client
        // so it can be reused for the next move.
        Ok(())
    }
}
```

- [ ] **Step 7: Add `pub mod lichess` to `src/esp32/mod.rs`**

```rust
pub mod lichess;
pub use lichess::{Esp32LichessClient, Esp32LichessError};
```

- [ ] **Step 8: Verify ESP32 build compiles**

Run: `just build`
Expected: Compiles without errors. (Cannot run tests on ESP32 target.)

---

### Task 7: Compile-time env var configuration

**Files:**
- Modify: `build.rs`
- Modify: `.env.example`

Add compile-time env var parsing for Lichess configuration. The token uses `option_env!()` (optional), while level/clock use `env!()` with defaults via `build.rs` `cargo:rustc-env`.

- [ ] **Step 1: Start change**

```bash
jj log -r @ --no-graph -T 'if(empty, "empty", "not-empty")' | grep -q "not-empty" && jj new
jj desc -m "feat: add compile-time Lichess env var configuration with validation"
```

- [ ] **Step 2: Update `.env.example`**

Add after the WiFi section:

```
# Lichess AI integration (optional — falls back to embedded AI without token)
# Generate a personal access token at https://lichess.org/account/oauth/token
# Required scopes: board:play, challenge:write
LICHESS_API_TOKEN=
LICHESS_AI_LEVEL=4
LICHESS_CLOCK_LIMIT=600
LICHESS_CLOCK_INCREMENT=0
```

- [ ] **Step 3: Update `build.rs` to emit default env vars**

```rust
fn main() {
    // Only run embuild when targeting ESP-IDF
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "espidf" {
        embuild::espidf::sysenv::output();
    }

    // Lichess config: emit defaults for optional env vars so env!() works at compile time.
    // If the user sets these in .env, their values take precedence.
    emit_default_env("LICHESS_AI_LEVEL", "4");
    emit_default_env("LICHESS_CLOCK_LIMIT", "600");
    emit_default_env("LICHESS_CLOCK_INCREMENT", "0");
}

fn emit_default_env(key: &str, default: &str) {
    if std::env::var(key).is_err() {
        println!("cargo:rustc-env={key}={default}");
    }

    // Validate at build time
    let value = std::env::var(key).unwrap_or_else(|_| default.to_string());
    match key {
        "LICHESS_AI_LEVEL" => {
            let v: u8 = value.parse().unwrap_or_else(|_| {
                panic!("{key}={value} is not a valid u8")
            });
            assert!(
                (1..=8).contains(&v),
                "{key}={v} is out of range (must be 1-8)"
            );
        }
        "LICHESS_CLOCK_LIMIT" | "LICHESS_CLOCK_INCREMENT" => {
            let _: u32 = value.parse().unwrap_or_else(|_| {
                panic!("{key}={value} is not a valid u32")
            });
        }
        _ => {}
    }

    // Re-run build script if this env var changes
    println!("cargo:rerun-if-env-changed={key}");
}
```

- [ ] **Step 4: Run tests to verify build still works**

Run: `just test`
Expected: All tests pass. Default values are emitted.

---

### Task 8: Wire up `main.rs` for Lichess opponent selection

**Files:**
- Modify: `src/main.rs`

After WiFi connects successfully, check for `LICHESS_API_TOKEN`. If present, construct `Esp32LichessClient`, call `spawn_lichess_opponent`, and use the result as the opponent. Fall back to `EmbeddedEngine` on any failure.

- [ ] **Step 1: Start change**

```bash
jj log -r @ --no-graph -T 'if(empty, "empty", "not-empty")' | grep -q "not-empty" && jj new
jj desc -m "feat: wire up Lichess opponent selection in ESP32 main with fallback"
```

- [ ] **Step 2: Update ESP32 `main()` to attempt Lichess connection**

After the WiFi success block and before the sensor setup, add:

```rust
let opponent: Box<dyn unnamed_chess_project::opponent::Opponent> =
    match option_env!("LICHESS_API_TOKEN") {
        Some(token) if _wifi.is_some() => {
            use unnamed_chess_project::esp32::Esp32LichessClient;
            use unnamed_chess_project::lichess::{LichessConfig, spawn_lichess_opponent};

            let config = LichessConfig {
                level: env!("LICHESS_AI_LEVEL").parse().unwrap(),
                clock_limit: env!("LICHESS_CLOCK_LIMIT").parse().unwrap(),
                clock_increment: env!("LICHESS_CLOCK_INCREMENT").parse().unwrap(),
            };

            let client = Esp32LichessClient::new(token);

            let spawn_fn = |f: Box<dyn FnOnce() + Send>| -> Result<(), String> {
                // FreeRTOS task spawn with 8KB stack
                std::thread::Builder::new()
                    .stack_size(8192)
                    .spawn(move || f())
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            };

            match spawn_lichess_opponent(client, config, spawn_fn) {
                Ok(lichess_opponent) => {
                    log::info!("Lichess opponent ready");
                    if let Err(e) =
                        display.show(&BoardFeedback::with_status(StatusKind::Success))
                    {
                        log::warn!("LED update failed: {e}");
                    }
                    FreeRtos::delay_ms(500);
                    Box::new(lichess_opponent)
                }
                Err(e) => {
                    log::warn!("Lichess setup failed: {e} — falling back to embedded AI");
                    if let Err(e) =
                        display.show(&BoardFeedback::with_status(StatusKind::Failure))
                    {
                        log::warn!("LED update failed: {e}");
                    }
                    FreeRtos::delay_ms(500);
                    Box::new(EmbeddedEngine::new(unsafe {
                        esp_idf_svc::sys::esp_random()
                    }))
                }
            }
        }
        _ => {
            log::info!("No Lichess token — using embedded AI");
            Box::new(EmbeddedEngine::new(unsafe {
                esp_idf_svc::sys::esp_random()
            }))
        }
    };
```

Then replace the existing `let opponent = EmbeddedEngine::new(...)` and `let mut session = GameSession::with_opponent(Box::new(opponent))` with:

```rust
let mut session = GameSession::with_opponent(opponent);
```

- [ ] **Step 3: Verify ESP32 build compiles**

Run: `just build`
Expected: Compiles without errors.

---

### Task 9: End-to-end smoke test on hardware (manual)

**Files:** None — this is a manual hardware validation step.

- [ ] **Step 1: Finalize the last change**

```bash
jj log -r @ --no-graph -T 'if(empty, "empty", "not-empty")' | grep -q "not-empty" && jj new
```

- [ ] **Step 2: Ensure `.env` has all required variables**

```
WIFI_SSID=<your-ssid>
WIFI_PASSWORD=<your-password>
LICHESS_API_TOKEN=lip_...
LICHESS_AI_LEVEL=4
LICHESS_CLOCK_LIMIT=600
LICHESS_CLOCK_INCREMENT=0
```

- [ ] **Step 3: Flash and monitor**

```bash
just flash
```

Expected serial output:
1. `WiFi connected` + `WiFi got IP: ...`
2. `Lichess challenge created: game_id=...`
3. `Lichess opponent ready`
4. Board shows success LED briefly, then clears

If Lichess setup fails, expect:
1. `Lichess setup failed: ... — falling back to embedded AI`
2. Board shows failure LED briefly, then continues with embedded engine

- [ ] **Step 4: Play a move and verify AI response**

Place pieces in starting position. Lift and place a white pawn (e.g. e2→e4). Monitor serial output for:
1. `Human plays: e2e4`
2. After a few seconds: `Computer plays: <move>` (the Lichess AI's response)
3. Board LEDs show recovery guidance for the AI's move (where to place/remove pieces)

- [ ] **Step 5: Test fallback — build without token**

Remove `LICHESS_API_TOKEN` from `.env` (or comment it out), rebuild and flash:

```bash
just flash
```

Expected: `No Lichess token — using embedded AI`. Board should work normally with the embedded heuristic engine.

---
