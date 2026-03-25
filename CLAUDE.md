# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ESP32-S3 smart chess board firmware in Rust. Hall-effect sensors detect per-color piece positions; LEDs provide move feedback. Uses `shakmaty` for chess logic. Supports an optional computer opponent (embedded heuristic engine or Lichess AI via HTTP).

## Build and Test Commands

The default Cargo target is `xtensa-esp32s3-espidf` (set in `.cargo/config.toml`). **You must always pass `--target` explicitly or use `just` commands to avoid accidentally targeting ESP32.**

```bash
just test              # Run all host tests
just test -- test_name # Run a single test by name
just dev               # Run interactive terminal simulator on host
just build             # Build ESP32 firmware (requires `cargo +esp`)
just flash             # Flash to ESP32 and monitor serial
```

### Linting

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --workspace --target $(rustc -vV | grep host | cut -d' ' -f2) -- -D warnings
```

CI treats clippy warnings as errors (`-D warnings`). ESP32 clippy uses `cargo +esp clippy`.

## Conditional Compilation

Two mutually exclusive code paths based on target:

| Context | Target | Active modules |
|---|---|---|
| ESP32-S3 firmware | `xtensa-esp32s3-espidf` (`target_os = "espidf"`) | `esp32::*` |
| Host (dev/test) | host triple (e.g. `aarch64-apple-darwin`) | `mock::*` |

Gate with `#[cfg(target_os = "espidf")]` / `#[cfg(not(target_os = "espidf"))]`. Never mix imports across the boundary. Tests use `#[cfg(all(test, not(target_os = "espidf")))]`.

## Architecture

### Data Flow

```
PieceSensor::read_positions() → ByColor<Bitboard>
    → GameEngine::tick(positions) → GameState
        → compute_feedback(&state) → BoardFeedback
            → BoardDisplay::show(&feedback)
```

**GameSession** (`session.rs`) orchestrates the full per-tick sequence shared by both the hardware loop and terminal simulator: `engine.tick()` → opponent handling → feedback computation → recovery fallback.

### Key Abstractions (traits in `lib.rs`)

- **PieceSensor** — sensor input (ESP32 hardware / mock scripted)
- **BoardDisplay** — visual output (ESP32 LEDs / terminal ANSI)
- **Opponent** (`opponent.rs`) — computer move selection (EmbeddedEngine / Lichess)
- **FeedbackSource** (`feedback.rs`) — decouples feedback computation from engine internals

### Module Responsibilities

- **game_logic.rs** — `GameEngine`: processes sensor bitboards, detects legal moves, advances chess position
- **feedback.rs** — `BoardFeedback`: maps game state to per-square LED instructions (Origin, Destination, Capture, Check, Checker)
- **session.rs** — `GameSession`: owns engine + opponent, produces `TickResult` per sensor frame
- **opponent.rs** — `Opponent` trait + `EmbeddedEngine` (heuristic: captures > castling > promotions > random)
- **lichess.rs** — Lichess API integration: challenge creation, NDJSON game stream, threaded opponent
- **recovery.rs** — highlights board divergence from expected position (guides player to fix misplaced pieces)
- **setup.rs** — pre-game feedback showing which starting-position squares still need pieces
- **mock/script.rs** — `ScriptedSensor` with BoardScript mini-language for tests

### Game Engine Constraints

- Promotions are always to Queen (no piece-selection mechanism on hardware)
- A move executes when pieces are placed, not just lifted (except en passant: 2 pieces lifted triggers processing)
- Illegal physical states are silently ignored; the engine waits for a valid position

### BoardScript Format (for tests)

Used by `ScriptedSensor` in `mock/script.rs` and extensively in `game_logic.rs` tests:

```
"e2 We4."      → lift e2, place white on e4, tick
"e2. We4."     → lift e2 (tick), place white on e4 (tick)
"e7 Be5. g1 Wf3."  → two moves in sequence
```

- Squares: `e2`, `a1`, `h8`. Optional `W`/`B` color prefix required when placing on an empty square.
- Period `.` flushes the current group and queues a tick.

## Environment Variables

WiFi credentials and Lichess config are compile-time `env!()` / `option_env!()` macros, loaded from `.env` via the justfile's `set dotenv-load`. Copy `.env.example` to `.env` and fill in values.

## Coding Conventions

- Error types: use `thiserror` with proper enums (never `()` as error type)
- No `unwrap()` in production code; `expect()` only where failure is logically impossible
- Prefer iterators over index-based loops, borrowing over cloning
- All public types implement `Debug` at minimum
- See `.github/instructions/rust.instructions.md` for full conventions
