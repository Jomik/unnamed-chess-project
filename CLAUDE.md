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
just fmt                # Format code
just clippy             # Run clippy on host (warnings are errors)
just check              # Run fmt + clippy + test
```

CI treats clippy warnings as errors (`-D warnings`). ESP32 clippy uses `cargo +esp clippy`.

## Conditional Compilation

Two mutually exclusive code paths based on target:

| Context | Target | Active modules |
|---|---|---|
| ESP32-S3 firmware | `xtensa-esp32s3-espidf` (`target_os = "espidf"`) | `esp32::*` |
| Host (dev/test) | host triple (e.g. `aarch64-apple-darwin`) | `mock::*` |

Gate with `#[cfg(target_os = "espidf")]` / `#[cfg(not(target_os = "espidf"))]`. Never mix imports across the boundary. Tests that import from `mock::*` use `#[cfg(all(test, not(target_os = "espidf")))]`; tests with no mock dependency use plain `#[cfg(test)]`.

## Architecture

### Data Flow

```
PieceSensor::read_positions() → ByColor<Bitboard>
    → GameSession::tick(sensors) → TickResult
        → Player::poll_move() detects/computes move
        → compute_feedback(position, prev_sensors, sensors) → BoardFeedback
            → BoardDisplay::show(&feedback)
```

**GameSession** (`session.rs`) orchestrates the per-tick sequence shared by both the hardware loop and terminal simulator: poll active player → apply move → notify opponent → compute feedback.

### Key Abstractions

- **PieceSensor** (`lib.rs`) — sensor input (ESP32 hardware / mock scripted)
- **BoardDisplay** (`lib.rs`) — visual output (ESP32 LEDs / terminal ANSI)
- **Player** (`player/mod.rs`) — symmetric trait for both human and computer players

### Module Responsibilities

- **player/mod.rs** — `Player` trait, `PlayerStatus` enum
- **player/human.rs** — `HumanPlayer`: detects moves from sensor bitboards by matching against legal moves
- **player/embedded.rs** — `EmbeddedEngine`: heuristic AI (captures > castling > promotions > random)
- **feedback.rs** — `compute_feedback`: pure function mapping (position, prev_sensors, curr_sensors) → per-square LED instructions. Recovery guidance is integrated as a fallback path.
- **session.rs** — `GameSession`: owns chess position + two `Box<dyn Player>`, produces `TickResult` per sensor frame
- **lichess.rs** — Lichess API integration: challenge creation, NDJSON game stream, `LichessOpponent` implements `Player`
- **setup.rs** — pre-game feedback showing which starting-position squares still need pieces
- **mock/script.rs** — `ScriptedSensor` with BoardScript mini-language for tests

### Move Detection Constraints

- Promotions are always to Queen (no piece-selection mechanism on hardware)
- A move executes when pieces are placed, not just lifted
- Illegal physical states are silently ignored; the player waits for a valid position

### BoardScript Format (for tests)

Used by `ScriptedSensor` in `mock/script.rs` and extensively in `player/human.rs` tests:

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
