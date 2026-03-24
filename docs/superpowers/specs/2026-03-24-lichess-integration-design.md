# Lichess AI Integration — Design Spec

**Date:** 2026-03-24
**Status:** Draft

## Goal

Enable the smart chess board to play against Lichess Stockfish AI over WiFi, using the Lichess Board API. When WiFi is available, the board creates a game against Lichess AI and streams moves in real time. When WiFi is unavailable, it falls back to the existing embedded heuristic engine.

## Constraints

- Player always plays White (simplification for v1)
- Standard chess variant only
- Compile-time configuration (token, AI level) via env vars
- No async runtime — ESP32 uses FreeRTOS tasks, host uses `std::thread`
- No new crate dependencies

## Lichess API Surface

All endpoints use `https://lichess.org` with a personal access token in the `Authorization: Bearer {token}` header.

Required OAuth2 scopes per endpoint:

| Endpoint | Required scopes |
|---|---|
| `POST /api/challenge/ai` | `board:play`, `challenge:write` (minimum); optionally `bot:play` — see scope note |
| `GET /api/board/game/stream/{gameId}` | `board:play` |
| `POST /api/board/game/{gameId}/move/{move}` | `board:play` |
| `POST /api/board/game/{gameId}/resign` | `board:play` (not used in v1) |

**Scope note:** The Lichess OpenAPI spec lists `challenge:write`, `bot:play`, and `board:play` under a single security entry for `POST /api/challenge/ai`. The [Lichess API authentication guide](https://github.com/lichess-org/api/blob/master/example/README.md) lists e-board apps as a personal token use case. `bot:play` is only for irreversibly upgraded Bot accounts and is not needed here.

**Required token scopes:** The Lichess OpenAPI spec lists `challenge:write`, `bot:play`, and `board:play` as the security requirement for `POST /api/challenge/ai`. **Token scope validation is the first implementation task** — before writing any other code, generate a personal access token with `board:play` + `challenge:write` and attempt a `POST /api/challenge/ai` call. If it succeeds, those two scopes are sufficient. If it returns 401/403, add `bot:play` to the token and retry. If `bot:play` requires a Bot account upgrade (irreversible), the contingency is to use a dedicated Bot account for the board — this is a product decision that must be made at that point, not in this spec. The implementation should log the full HTTP response on auth failure to aid diagnosis.

| Endpoint | Purpose | Key details |
|---|---|---|
| `POST /api/challenge/ai` | Start a game vs Stockfish | Form-encoded; required: `level` (1-8); optional: `clock.limit`, `clock.increment`, `color`, `variant`. Returns HTTP `201` with JSON body containing `id` (8-char game ID). |
| `GET /api/board/game/stream/{gameId}` | Stream game state | NDJSON stream. First line: `gameFull` (immutable game data + initial `state`). Subsequent lines: `gameState` (moves as space-separated UCI, status, winner). Server closes stream when game ends. |
| `POST /api/board/game/{gameId}/move/{move}` | Send a move | Move in UCI format as path param (e.g. `e2e4`). Returns `{"ok": true}`. |
| `POST /api/board/game/{gameId}/resign` | Resign the game | No body. Returns `{"ok": true}`. Not used in v1 — Lichess ends the game automatically on checkmate/stalemate. |

### Relevant `gameState` fields

- `moves`: space-separated UCI string of all moves played so far
- `status`: one of `started`, `mate`, `resign`, `stalemate`, `draw`, `aborted`, `timeout`, `outoftime`, and others
- `winner`: optional, `"white"` or `"black"`

### Relevant `gameFull` fields

- `id`: 8-char game ID
- `initialFen`: `"startpos"` or a FEN string
- `white`/`black`: player objects (with optional `aiLevel` field for Stockfish)
- `state`: embedded `gameState` object with initial state

## Architecture

### Module layout

```
src/
  lichess.rs          # Platform-independent: LichessClient/LichessGame/LichessStream
                      # traits, types, LichessOpponent, spawn_lichess_opponent()
  opponent.rs         # Modified: poll_move gains position param, has_error() added
  session.rs          # Modified: passes position to poll_move, checks has_error()
  esp32/
    lichess.rs        # ESP32 impls of LichessClient/LichessGame/LichessStream
    mod.rs            # Re-exports esp32::lichess
  mock/
    lichess.rs        # Host stub impls (real networking deferred to later)
    mod.rs            # Re-exports mock::lichess
  main.rs             # Modified: opponent selection (WiFi + Lichess, else embedded)
  lib.rs              # Modified: adds pub mod lichess
```

### `LichessClient`, `LichessGame`, and `LichessStream` traits

Defined in `src/lichess.rs`. Split into two traits to separate the startup phase (calling thread) from the in-game phase (background worker).

**Key constraint:** On ESP32, `EspHttpConnection` is `!Send` — HTTP connections cannot be moved across thread/task boundaries. The design avoids this by ensuring all HTTP connections are created on the thread that uses them. The `LichessGame` handle moved into the worker contains only a game ID string and config — no HTTP connection.

```rust
/// Used on the calling thread during startup.
/// Implementations may be !Send.
pub trait LichessClient {
    type Error: std::fmt::Debug + std::fmt::Display;
    type Game: LichessGame<Error = Self::Error> + Send + 'static;

    /// Creates a game against Lichess Stockfish. The HTTP impl must send:
    /// - `level` = the given level
    /// - `color=white` (player always plays White in v1)
    /// - `variant=standard`
    /// - `clock.limit` and `clock.increment` as given
    /// Returns a `LichessGame` handle for the created game on success.
    /// The handle holds only the game ID and config — no HTTP connection.
    fn challenge_ai(
        self,
        level: u8,
        clock_limit: u32,
        clock_increment: u32,
    ) -> Result<Self::Game, Self::Error>;
}

/// Moved into the background worker. Must be Send.
/// Holds the game ID and enough context to open HTTP connections
/// on the worker thread — does NOT hold an open connection itself.
pub trait LichessGame: Send + 'static {
    type Error: std::fmt::Debug + std::fmt::Display;

    /// Returns the 8-char game ID.
    fn game_id(&self) -> &str;

    /// Opens the NDJSON stream and returns a LichessStream.
    /// Called on the worker thread — all HTTP connections are created here.
    /// Consumes `self`.
    fn into_stream(self) -> Result<Box<dyn LichessStream<Error = Self::Error>>, Self::Error>;
}

/// Owned by the background worker. Created on the worker thread.
/// May be !Send — it is never moved after creation.
pub trait LichessStream {
    type Error: std::fmt::Debug + std::fmt::Display;

    /// Read the next event. Blocks until an event arrives, the 60s
    /// timeout elapses, or the stream closes. Returns None on close/timeout.
    fn next_event(&mut self) -> Option<Result<GameEvent, Self::Error>>;

    /// Send a player move in UCI format.
    fn make_move(&mut self, uci_move: &str) -> Result<(), Self::Error>;
}
```

The platform-specific impl (`Esp32LichessClient`, `MockLichessClient`) implements `LichessClient`. `challenge_ai` makes one HTTP POST and returns a `LichessGame` impl holding only the game ID, token, and config. `into_stream()` is called on the worker thread and opens two HTTP connections: one for the NDJSON stream, one for POSTs.

### Types (in `src/lichess.rs`)

```rust
pub enum GameEvent {
    GameFull {
        id: String,
        initial_fen: String,
        state: GameStateData,
    },
    GameState(GameStateData),
    // chatLine and opponentGone events from the Lichess stream are parsed
    // and silently discarded by the LichessHttp impl — they are never
    // yielded through the iterator. The iterator only yields GameFull
    // and GameState events.
}

pub struct GameStateData {
    pub moves: String,        // space-separated UCI
    pub status: GameStatus,
    pub winner: Option<Color>,
}

pub enum GameStatus {
    Started,
    Mate,
    Resign,
    Stalemate,
    Draw,
    Aborted,
    Timeout,    // "timeout" — player ran out of time
    Outoftime,  // "outoftime" — Lichess's alternate name for the same condition
    Other(String),
}

pub struct LichessConfig {
    pub token: &'static str,   // LICHESS_API_TOKEN — static str from env!()
    pub level: u8,             // LICHESS_AI_LEVEL, 1-8, default 4
    pub clock_limit: u32,      // LICHESS_CLOCK_LIMIT, seconds, default 10800 (3 hours)
    pub clock_increment: u32,  // LICHESS_CLOCK_INCREMENT, seconds, default 180 (3 min)
}
```

`LichessConfig` is constructed in `main.rs` and passed to `spawn_lichess_opponent`. The token is a `&'static str` (from `env!()`) and is passed through `LichessConfig` to `LichessClient` and then to `LichessGame` (which holds it for use in `into_stream`). The token is not duplicated — it flows through the chain as a reference.

**Invalid env var values:** `LICHESS_AI_LEVEL`, `LICHESS_CLOCK_LIMIT`, and `LICHESS_CLOCK_INCREMENT` are parsed at compile time. If a value cannot be parsed as the expected integer type, the build fails with a compile error. If the value is out of range (e.g. `LICHESS_AI_LEVEL=9`), the build also fails with a compile error via a `const` assertion — values are not silently clamped.

```rust

pub enum LichessMessage {
    AiMove(String),   // UCI move string from the AI
    GameOver,         // game ended normally (checkmate, stalemate, draw, etc.)
    Error(String),    // mid-game failure (network drop, parse error, etc.)
}
```

### `LichessOpponent` struct

```rust
pub struct LichessOpponent {
    player_move_tx: SyncSender<String>,  // capacity 1 — game is turn-based
    ai_move_rx: Receiver<LichessMessage>,
    error: bool,
}
```

Does not own any `LichessHttp` impl — that lives entirely in the background worker. `LichessOpponent` only holds channel ends. After `spawn_lichess_opponent` returns, the calling thread holds no reference to the HTTP client, game handle, or stream.

### Non-blocking design

```
spawn_lichess_opponent()         Background worker (spawned thread)
========================         ==================================
client.challenge_ai() -> game    game.into_stream() -> stream
spawn(worker, game, channels)    stream.next_event() -> GameFull
return LichessOpponent           loop:
                                   recv player_move from channel
Game loop (main thread)            stream.make_move(uci)
========================           stream.next_event() -> GameState
start_thinking():                  extract AI's last move (by move count)
  send UCI via player_move_tx -->  send AI move via ai_move_tx
poll_move():              <--      if status != Started, break
  try_recv on ai_move_rx
  parse UCI -> shakmaty::Move
```

**Move count tracking:** The `moves` field in every `gameState` is the complete space-separated UCI history of all moves played so far. The worker tracks `expected_move_count` (incremented by 2 each round: +1 for the player's move, +1 for the AI's reply). For each `gameState` received:

1. If `moves.split_whitespace().count() < expected_move_count`:
   - If `status != Started`: the human's move ended the game (checkmate, stalemate, etc.) — no AI reply will arrive. Send `LichessMessage::GameOver` and exit the worker.
   - If `status == Started`: intermediate acknowledgement or draw-offer update — discard and continue reading.
2. If `moves.split_whitespace().count() == expected_move_count`:
   - The last token in `moves` is the AI's move — send `LichessMessage::AiMove(uci)`.
   - If `status != Started`: the AI's move ended the game — send `LichessMessage::GameOver` and exit the worker.
   - If `status == Started`: normal mid-game reply — wait for the next player move.

If the stream closes unexpectedly (`next_event` returns `None` before a terminal status), the worker sends `LichessMessage::Error("stream closed unexpectedly")` and exits. This covers all terminal states including those not explicitly modelled in the `GameStatus` enum (captured by `Other(String)`).

### `spawn_lichess_opponent` function

```rust
pub fn spawn_lichess_opponent<C, E>(
    client: C,
    config: LichessConfig,
    spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> Result<(), E>,
) -> Result<LichessOpponent, SpawnError<C::Error, E>>
where
    C: LichessClient,
    E: std::fmt::Debug + std::fmt::Display,
```

Where `SpawnError` is:

```rust
pub enum SpawnError<H, S> {
    Http(H),                  // challenge_ai failed on the calling thread
    Spawn(S),                 // thread/task creation failed
    WorkerStartup(String),    // worker failed to open stream or read GameFull;
                              // error is a human-readable description string
}
```

**Startup sequence:**

On the calling thread:
1. Call `client.challenge_ai(...)` → get `game: LichessGame` (holds game ID + config, no HTTP connection)
   - On failure: return `Err(SpawnError::Http(...))`
2. Create the move channels (`SyncSender`/`Receiver` pair, capacity 1)
3. Create a one-shot ready channel carrying `Result<(), String>`: `(ready_tx, ready_rx)`
4. Build the worker closure, moving `game`, move channels, and `ready_tx` into it
5. Call `spawn(worker_closure)`:
   - On spawn failure: `game` has been moved into the closure and is no longer accessible to the caller. The orphaned Lichess game will time out naturally (Lichess aborts games where neither player moves within the time control). Return `Err(SpawnError::Spawn(...))`.
6. Block on `ready_rx.recv()` — wait for the worker to signal ready or error:
   - `Ok(Ok(()))`: worker started successfully → return `LichessOpponent` holding the channel ends
   - `Ok(Err(description))`: worker startup failed → return `Err(SpawnError::WorkerStartup(description))`
   - `Err(_)` (sender dropped, e.g. worker panicked): return `Err(SpawnError::WorkerStartup("worker exited before signalling ready".into()))`

On the worker thread (inside the closure):
1. Call `game.into_stream()` → opens the stream connection and a POST client; returns `LichessStream`
   - On failure: send `Err(description)` through `ready_tx`, exit
2. Call `stream.next_event()` → must be `GameFull`; initialize `expected_move_count` from `gameFull.state.moves.split_whitespace().count()`
   - On failure or wrong event type: send `Err(description)` through `ready_tx`, exit
3. Send `Ok(())` through `ready_tx` — signals the calling thread that startup succeeded
4. Enter the main game loop (see Non-blocking design)

The `spawn` parameter abstracts thread creation: ESP32 passes a FreeRTOS task spawner, host passes `std::thread::spawn`. The ready channel uses `std::sync::mpsc` (capacity 1, send-once).

### `Opponent` trait changes

```rust
pub trait Opponent {
    fn start_thinking(&mut self, position: &Chess, human_move: &Move);
    fn poll_move(&mut self, position: &Chess) -> Option<Move>;  // CHANGED: added position param
    fn has_error(&self) -> bool { false }                        // NEW: default no-op
}
```

- `poll_move` gains `position: &Chess` so `LichessOpponent` can parse UCI into a `shakmaty::Move` without storing stale position state
- `has_error` has a default `false` impl so `EmbeddedEngine` needs no changes

### `GameSession` changes

`handle_opponent` is split into two responsibilities:

1. **On human move** (`state.human_move()` is `Some`): call `opponent.start_thinking(position, human_move)` to notify the opponent of the player's move and begin waiting for a reply.

2. **Every tick** (regardless of whether a human move occurred): call `opponent.poll_move(position)` to check for a pending AI reply. This is necessary because the Lichess AI reply arrives asynchronously — potentially many ticks after `start_thinking` was called. `EmbeddedEngine` returns `None` on ticks where it has no pending move, so this is safe for both impls.

3. **Every tick**: check `opponent.has_error()` and merge `StatusKind::Failure` into the tick's feedback when true.

### `main.rs` changes (ESP32)

After WiFi connects successfully:
1. Construct `LichessConfig` from compile-time env vars
2. Construct ESP32 `LichessHttp` impl
3. Call `spawn_lichess_opponent` with a FreeRTOS task spawner
4. On success: use `LichessOpponent` as the opponent
5. On failure: log warning, show failure LED briefly, fall back to `EmbeddedEngine`

### Compile-time configuration

| Env var | Required | Default | Description |
|---|---|---|---|
| `LICHESS_API_TOKEN` | No (falls back to embedded AI) | - | Personal access token; required to enable Lichess mode. See scope note above. |
| `LICHESS_AI_LEVEL` | No | `4` | Stockfish level 1-8 |
| `LICHESS_CLOCK_LIMIT` | No | `10800` | Clock initial time in seconds (3 hours — board has no clock display) |
| `LICHESS_CLOCK_INCREMENT` | No | `180` | Clock increment in seconds (3 min per move) |

Same `env!()` / `option_env!()` pattern as `WIFI_SSID` / `WIFI_PASSWORD`.

Note: `LICHESS_API_TOKEN` is embedded in the firmware binary at compile time, same as WiFi credentials. This is intentional for personal/local builds only — the token must never be committed to source control or shared. Treat it as a per-device secret. This approach is not suitable for distributed firmware builds.

## Error handling

**Startup failures**: `spawn_lichess_opponent` returns `Err` in three cases:
- `challenge_ai` fails (HTTP error, auth failure, etc.) → `SpawnError::Http`
- `spawn` fails (FreeRTOS out of stack, etc.) → `SpawnError::Spawn`; the orphaned Lichess game times out naturally
- Worker startup fails (`into_stream` or first `GameFull` event fails, or worker panics) → `SpawnError::WorkerStartup(String)`

In all cases, `main.rs` falls back to `EmbeddedEngine`. Board shows `StatusKind::Failure` LED briefly.

**Normal game completion**: Worker sends `LichessMessage::GameOver` when the game ends normally (any terminal `gameState.status`). `LichessOpponent::poll_move` receives `GameOver` and returns `None`. `has_error()` remains `false`. The board continues showing its last feedback state — no failure LED.

**Mid-game failures** (network drop, HTTP error): Background worker sends `LichessMessage::Error` through the channel, then exits. `LichessOpponent::poll_move` checks for `LichessMessage::Error` and sets `error = true`. If the worker exits without sending any message (channel disconnected unexpectedly), `poll_move` detects the disconnected channel via `try_recv` returning `Err(TryRecvError::Disconnected)` and also sets `error = true`. In both cases, `has_error()` returns `true`, `GameSession` merges `StatusKind::Failure` into feedback, and no further moves are processed. Board shows failure LED until power-cycled.

**Stalled stream:** A stream that stops producing events (dead connection, no close) is treated as a mid-game failure. The 60-second `timeout_ms` on the stream HTTP client (see HTTP timeouts above) causes `next_event()` to return `None` after 60 seconds of inactivity. The worker treats this as a mid-game error, sends `LichessMessage::Error`, and exits. This is safe for the default 10-minute clock: Lichess Stockfish typically replies in under 5 seconds, so 60 seconds is far above any normal AI thinking time while still detecting dead connections promptly.

**HTTP connections:** Inside `into_stream()` on the worker thread, the `LichessStream` impl creates:
- One persistent HTTP connection for the NDJSON stream (long-lived, kept open until the game ends or times out)
- One HTTP client for `make_move` POSTs (created once, reused for each POST request)

Both are owned by the `LichessStream` impl and never cross a thread boundary.

**Invalid AI move** (shouldn't happen): UCI parse failure against current position is treated as a terminal error — `LichessOpponent` sets `error = true` and `has_error()` returns `true`. The board shows the failure LED. This avoids a silent hang where the game loop waits indefinitely for a valid move that will never arrive.

**HTTP timeouts:** The ESP32 `LichessClient` and `LichessStream` impls must configure timeouts explicitly on the ESP-IDF HTTP client (`timeout_ms` field in `EspHttpConnectionConfiguration`):
- Startup requests (`challenge_ai`): `timeout_ms = 10_000` (10 seconds)
- Stream connection (`into_stream`, opening the NDJSON stream): `timeout_ms = 10_000` (10 seconds for the initial connection)
- Stream read (steady-state, waiting for the next `gameState` event): the stream connection must be configured with `timeout_ms = 60_000` (60 seconds) to allow the AI time to reply without triggering a spurious timeout. This is set on the stream HTTP client, not the startup client.

**HTTPS certificate verification (ESP32):** The ESP32 `LichessClient` impl uses the ESP-IDF CA bundle (`esp_crt_bundle_attach`) to verify `https://lichess.org`. This is the standard approach in `esp-idf-svc` and requires no additional certificates to be bundled manually. Certificate verification must not be disabled — connecting without verification would expose the token.

**Missing `LICHESS_API_TOKEN`:** If `LICHESS_API_TOKEN` is not set at compile time and WiFi is enabled, the build uses `option_env!()` to detect the absence and falls back to `EmbeddedEngine` at runtime (same as WiFi failure). No compile error — the board remains functional without a token.

## Out of scope (v1)

- Playing as Black
- Game resumption after network drop
- Multiple games per session
- Time control display on LEDs
- Draw offers / takebacks
- Host `LichessHttp` impl (stub only — real impl deferred)
- Non-standard variants
