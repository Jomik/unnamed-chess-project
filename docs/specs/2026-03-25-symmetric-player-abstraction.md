# Symmetric Player Abstraction

## Problem

The current architecture has an asymmetry at its core: the human player is implicit (embedded in `GameEngine::process_moves` and the sensor loop) while the computer opponent has an explicit `Opponent` trait. This creates several issues:

- **GameEngine conflates two concerns**: chess position management and human move detection from sensor bitboards. These are fundamentally different responsibilities.
- **GameSession manually orchestrates a fragile chain**: `engine.tick()` → opponent handling → `compute_feedback()` → empty check → `recovery_feedback()`. The feedback fallback logic is behavioral but lives in the caller, not encapsulated.
- **FeedbackSource trait is shallow**: its 6 methods mirror `GameState` fields 1:1 with zero transformation. The `MockFeedbackSource` in tests duplicates `GameState` field-by-field.
- **GameState is a pass-through struct**: it exists solely to shuttle data from `GameEngine` to `compute_feedback`, with no logic of its own beyond implementing `FeedbackSource`.

## Design

### Player Trait

A single trait that both human and computer players implement:

```rust
enum PlayerStatus {
    Active,
    Error,
    GameOver,
}

trait Player {
    /// Return a move if one is detected/ready. Called every tick for the active player.
    fn poll_move(&mut self, position: &Chess, sensors: ByColor<Bitboard>) -> Option<Move>;

    /// Notification that the opponent just played. Agents start thinking here.
    fn opponent_moved(&mut self, position: &Chess, opponent_move: &Move);

    /// Player health. Checked by session each tick.
    fn status(&self) -> PlayerStatus { PlayerStatus::Active }
}
```

**Design decisions:**

- `sensors` is passed to `poll_move` rather than split into a separate `observe()` step. Agents ignore it. This avoids an implicit ordering dependency between two calls.
- Returns `Option<Move>` rather than a richer `PollResult`/`GameAction` enum. Game actions (resign, draw offers) can be added as an enum later when needed — right now only moves exist.
- `PlayerStatus` replaces `has_error() -> bool`, adding `GameOver` to represent external game termination (Lichess resign/timeout) which is distinct from errors.

### HumanPlayer

Extracts move detection logic from `GameEngine::process_moves()`:

```rust
struct HumanPlayer {
    last_sensors: ByColor<Bitboard>,
}
```

- `poll_move`: compares `sensors` against `last_sensors` and the chess position's expected board state. Finds the legal move whose resulting board matches the physical sensor state. Returns `Some(mv)` when a valid move is detected.
- `opponent_moved`: no-op. Recovery LEDs (computed by the feedback system) guide the human to physically execute the opponent's move.
- Does not own the chess position — borrows `&Chess` from the session.
- The `human_color` field in the current `GameEngine` becomes unnecessary: the session only polls the player whose turn it is.
- When the board is in recovery (pieces misplaced after computer move), no legal move will match the physical state, so `poll_move` naturally returns `None`.

### Agent Implementations

**EmbeddedEngine** implements `Player` directly:
- `opponent_moved` → picks a move immediately using the existing heuristic (captures > castling > promotions > random). Was `start_thinking`.
- `poll_move` → returns `self.pending.take()`. Ignores sensors.

**LichessOpponent** implements `Player` directly:
- `opponent_moved` → sends human's UCI move to the Lichess worker thread. Was `start_thinking`.
- `poll_move` → polls the channel for the AI's reply. Ignores sensors.
- `status()` → maps internal `error`/`game_over` booleans to `PlayerStatus`.

The `Opponent` trait is deleted. The worker loop, channel bridge, and `spawn_lichess_opponent` are unchanged.

### GameSession

```rust
struct GameSession {
    position: Chess,
    white: Box<dyn Player>,
    black: Box<dyn Player>,
    prev_sensors: ByColor<Bitboard>,
}
```

The session owns the chess position (previously owned by `GameEngine`) and two players. Each tick:

1. Read sensors (done by caller, passed in)
2. Poll the active player: `current_player.poll_move(&position, sensors)`
3. If `Some(mv)`: validate legality, apply to position, call `other_player.opponent_moved(&self.position, &mv)` — note: the position passed is the **post-move** state, so the agent sees the board after the human's move has been applied.
4. Check both players' `status()` for errors/game-over. If either player returns `Error`, merge `StatusKind::Failure` into feedback. If either returns `GameOver`, do not merge any status indicator — the chess position's own `GameOutcome` (checkmate/stalemate) already drives the LED display via `compute_feedback`. For external terminations (Lichess resign/timeout) where the position has no outcome, the feedback simply goes empty (no LEDs). The player self-gates: once `status()` returns `GameOver`, `poll_move` returns `None` on all subsequent ticks.
5. Compute feedback from `(position, prev_sensors, sensors)`
6. Update `prev_sensors`
7. Return `TickResult`

`TickResult` after the refactor:

```rust
struct TickResult {
    /// Computed board feedback (move guidance, recovery, status).
    pub feedback: BoardFeedback,
    /// The move committed during this tick, if any (by either player).
    pub last_move: Option<Move>,
}
```

`GameState` is deleted — its fields were only needed to feed `FeedbackSource`, which is also deleted. The `computer_move` field is replaced by `last_move`, which covers moves from either player. The main loop uses `last_move` for logging.

The caller (main loop) reads the sensor and passes positions in. The session stays pure — no I/O, no sensor ownership. This keeps testing trivial (pass bitboard data directly) and lets the caller handle sensor errors, timing, and diagnostic logging.

#### No-opponent mode

The current `GameSession::new()` supports a mode with no computer opponent (both sides are human-detected from the board). In the new design, both player slots must be filled. For no-opponent mode, both `white` and `black` are `HumanPlayer` instances — the session polls whichever player's turn it is, and both detect moves from the same sensor. This preserves the existing behavior where either color's moves are detected from the board.

### Feedback Computation

`compute_feedback` becomes a pure function with no trait or intermediate state struct:

```rust
fn compute_feedback(
    position: &Chess,
    prev_sensors: ByColor<Bitboard>,
    curr_sensors: ByColor<Bitboard>,
) -> BoardFeedback
```

It derives all feedback internally:

- **Lifted piece**: `position.us() & !curr_combined` — our piece missing from the physical board.
- **Captured piece**: `position.them() & !curr_combined & prev_combined` — opponent piece physically present last tick but gone now.
- **Legal destinations**: from `position.legal_moves()`, filtered by lifted/captured context.
- **Check/checker**: from `position.checkers()` and king square.
- **Game outcome**: no legal moves → checkmate (if in check) or stalemate.
- **Recovery guidance**: `expected_board ^ physical_board` — when the board diverges from the expected position (e.g., after a computer move), show where pieces need to go.
- **Castle rook guidance**: expected rook on target square vs physical rook still on origin.
- **In-recovery suppression**: after computing lifted and captured squares, check whether the board has *additional* divergence beyond those squares (unexpected occupancy differences or wrong-color pieces). If so, the board is in recovery — zero out lifted piece, captured piece, and check feedback, and show only recovery guidance. This prevents move-destination highlights and check highlights from co-appearing with recovery highlights, matching the existing behavior in `GameEngine::tick()`.

The following are deleted:
- `FeedbackSource` trait
- `GameState` struct (and its `FeedbackSource` impl)
- `MockFeedbackSource` (used in feedback tests)
- `recovery_feedback` as a separate function (merged into `compute_feedback`)

### Main Loop

The main loop structure stays the same for both ESP32 and terminal simulator:

```rust
loop {
    let positions = sensor.read_positions()?;
    let result = session.tick(positions);
    display.show(&result.feedback)?;
}
```

The terminal simulator constructs a `GameSession` with a `HumanPlayer` and an `EmbeddedEngine` (or `LichessOpponent`). The ESP32 main does the same. Setup phase (`setup_feedback`) runs before the session is created, unchanged.

The terminal simulator's `draw_dual_boards` currently calls `session.engine()` to access `GameEngine::piece_at()` for rendering piece symbols. After `GameEngine` is deleted, the session exposes `session.position()` returning `&Chess`, and the terminal uses `position.board().piece_at(square)` instead. This is a direct replacement with the same underlying data.

## Module Map

| Current | After |
|---|---|
| `game_logic.rs` (GameEngine, GameState) | Deleted. Move detection → `HumanPlayer`. Position management → `GameSession`. |
| `feedback.rs` (FeedbackSource, compute_feedback) | Simplified. `FeedbackSource` deleted. `compute_feedback` is a pure function. Recovery merged in. |
| `recovery.rs` | Merged into `feedback.rs`. File deleted. |
| `opponent.rs` (Opponent trait, EmbeddedEngine) | Becomes `player.rs`. `Opponent` → `Player` trait. `EmbeddedEngine` implements `Player`. |
| `session.rs` | Refactored. Owns `Chess` position + two `Box<dyn Player>` + `prev_sensors`. |
| `lichess.rs` | Minimal change. `LichessOpponent` implements `Player` instead of `Opponent`. |
| `setup.rs` | Unchanged. |
| `mock/*`, `esp32/*` | Unchanged (except terminal simulator wiring). |
| `lib.rs` | Exports `Player`, `PlayerStatus`. `PieceSensor` and `BoardDisplay` stay. |

## Implementation Plan

Four incremental changes. The codebase compiles and all tests pass after each step.

### Step 1: Introduce Player trait + HumanPlayer (additive)

- New: `Player` trait, `PlayerStatus` enum, `HumanPlayer` struct
- `HumanPlayer` extracts move detection logic from `GameEngine::process_moves`
- Full test suite for `HumanPlayer`, ported from `game_logic.rs` move detection tests
- Nothing deleted — `GameEngine`, `Opponent`, `GameSession` continue to work

### Step 2: Agents implement Player, delete Opponent trait

- `EmbeddedEngine` implements `Player` (`start_thinking` → `opponent_moved`, `poll_move` gains unused `sensors`)
- `LichessOpponent` implements `Player`, `status()` replaces `has_error()`
- `Opponent` trait deleted
- Existing agent tests adapted

### Step 3: Pure feedback function, merge recovery

- `compute_feedback(position, prev_sensors, curr_sensors)` — new signature
- `recovery_feedback` merged into `compute_feedback` as fallback path
- `FeedbackSource` trait, `MockFeedbackSource`, `GameState` (as FeedbackSource impl) deleted
- Session temporarily adapts to call the new function
- Feedback and integration tests rewritten

### Step 4: GameSession uses Players, delete GameEngine

- Session takes `Box<dyn Player>` for white and black, owns `Chess` position + `prev_sensors`
- `GameEngine` and `GameState` deleted
- Session tests rewritten
- Main loop and terminal simulator updated

## Testing Strategy

Existing test scenarios remain valid — they are restructured, not discarded:

| Current | After |
|---|---|
| `game_logic.rs` tests (move detection, castling, en passant, etc.) | `HumanPlayer` tests using the same `ScriptedSensor`/BoardScript approach |
| `feedback.rs` tests with `MockFeedbackSource` | Tests calling `compute_feedback(position, prev, curr)` directly — no mock needed |
| `recovery.rs` tests | Merged into feedback tests (recovery is now a code path in `compute_feedback`) |
| `session.rs` tests | Session tests with `HumanPlayer` + mock agents |
| `tests/feedback_integration.rs` | Simplified — feedback is one function call, integration is naturally tested through session |
| `opponent.rs` tests | `EmbeddedEngine` tests, method names updated |
| `lichess.rs` tests | Unchanged (MockStream/MockGame/MockClient stay) |
