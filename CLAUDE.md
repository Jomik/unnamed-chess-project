# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ESP32-S3 smart chess board firmware in Rust, plus an iOS companion app (SwiftUI + CoreBluetooth). Hall-effect sensors detect per-color piece positions; LEDs provide move feedback. Uses `shakmaty` for chess logic. The companion app connects via BLE to configure players, send WiFi/Lichess credentials, and start games. Supports human-vs-human, human-vs-embedded-engine, and human-vs-Lichess-AI modes.

## Build and Test Commands

The default Cargo target is `xtensa-esp32s3-espidf` (set in `.cargo/config.toml`). **You must always pass `--target` explicitly or use `just` commands to avoid accidentally targeting ESP32.**

```bash
just test              # Run all host tests
just test -- test_name # Run a single test by name
just build             # Build ESP32 firmware
just build-diag        # Build diagnostics binary
just flash             # Flash to ESP32 and monitor serial
just flash-diag        # Flash diagnostics binary and monitor serial
```

### iOS Companion App

The companion app is at `companion/ChessBoard/`. It uses XcodeGen to generate the Xcode project from `project.yml`.

```bash
just companion-build   # Generate .xcodeproj + build for simulator
just companion-test    # Generate .xcodeproj + run tests
just companion-open    # Generate .xcodeproj + open in Xcode
```

Requires Xcode 15+ and XcodeGen (`brew install xcodegen`). BLE does not work in the simulator — use a physical device for end-to-end testing.

For physical device deployment, copy `companion/ChessBoard/Local.xcconfig.template` to `Local.xcconfig` and set your `DEVELOPMENT_TEAM` ID.

### Linting

```bash
just fmt                # Format code
just clippy             # Run clippy on host (warnings are errors)
just clippy-esp         # Run clippy for ESP32 target
just check              # Run fmt + clippy + test (host only)
just check-all          # Run check + clippy-esp
```

CI treats clippy warnings as errors (`-D warnings`). **Never run `cargo +esp` directly** — always use `just` recipes, which load `IDF_PATH` from `.env` via `set dotenv-load`. Running `cargo +esp` without `IDF_PATH` corrupts the ESP-IDF setup.

## Conditional Compilation

Two mutually exclusive code paths based on target:

| Context | Target | Active modules |
|---|---|---|
| ESP32-S3 firmware | `xtensa-esp32s3-espidf` (`target_os = "espidf"`) | `esp32::*` |
| Host (dev/test) | host triple (e.g. `aarch64-apple-darwin`) | `testutil::*` (test-only) |

Gate hardware-specific code with `#[cfg(target_os = "espidf")]`. Never mix imports across the boundary. All test modules use plain `#[cfg(test)]`.

## Architecture

### Data Flow

```
BleCommands (mpsc channel) → BleCommand
    → main.rs drains commands each tick (start game, resign, query state)
    → BleNotifier pushes state back to characteristics
PieceSensor::read_positions() → ByColor<Bitboard>
    → GameSession::tick(sensors) → TickResult
        → Player::poll_move() detects/computes move
        → compute_feedback(position, sensors, reference_sensors) → BoardFeedback
            → BoardDisplay::show(&feedback)
```

**GameSession** (`session.rs`) orchestrates the per-tick sequence: poll active player → apply move → notify opponent → compute feedback. `main.rs` also drains `BleCommand`s from the BLE server each tick before calling `session.tick()`.

### Key Abstractions

- **PieceSensor** (`lib.rs`) — sensor input (ESP32 hardware / test scripted)
- **BoardDisplay** (`lib.rs`) — visual output (ESP32 LEDs)
- **Player** (`player/mod.rs`) — symmetric trait for both human and computer players

### Module Responsibilities

- **player/mod.rs** — `Player` trait (`poll_move`, `opponent_moved`, `is_interactive`), `PlayerStatus` enum
- **player/human.rs** — `HumanPlayer`: detects moves from sensor bitboards by matching against legal moves
- **player/embedded.rs** — `EmbeddedEngine`: heuristic AI (captures > castling > promotions > random)
- **feedback.rs** — `compute_feedback` and `compute_state_feedback`: feedback from position + sensors. Recovery guidance is integrated as a fallback path.
- **session.rs** — `GameSession`: owns chess position + two `Box<dyn Player>`, produces `TickResult` per sensor frame; also exposes `resign()`, `is_game_over()`, and `game_state()` for BLE game lifecycle management
- **ble_protocol.rs** — `BleCommand`, `PlayerConfig`, `GameState`, `CommandResult`, `WifiAuthMode`, `WifiConfig`, `WifiState`, `WifiStatus`, `LichessState`, `LichessStatus`, UUID constants, binary encoding/decoding. Platform-independent, host-testable.
- **esp32/sensor.rs** — `Esp32PieceSensor`: ADC + mux scanning, `RawScan` for raw millivolt readings, `read_raw()` primitive
- **esp32/ble.rs** — `start_ble()` initializes NimBLE and returns `BleCommands` (command receiver) + `BleNotifier` (characteristic updater). Three fully functional GATT services (WiFi, Lichess, Game), typed characteristic handles.
- **esp32/config.rs** — `SensorCalibration` NVS load/save (cal partition), `CalibrationError`, `SensorConfig`, `LedPalette`, `Rgb8` display/sensor configuration types
- **lichess.rs** — Lichess API integration: challenge creation, NDJSON game stream, `LichessOpponent` implements `Player`
- **setup.rs** — pre-game feedback showing which starting-position squares still need pieces
- **testutil/script.rs** — `ScriptedSensor` with BoardScript mini-language for tests

### Move Detection Constraints

- Promotions are always to Queen (no piece-selection mechanism on hardware)
- A move executes when pieces are placed, not just lifted
- Illegal physical states are silently ignored; the player waits for a valid position

### BoardScript Format (for tests)

Used by `ScriptedSensor` in `testutil/script.rs` and extensively in `player/human.rs` tests:

```
"e2 We4."      → lift e2, place white on e4, tick
"e2. We4."     → lift e2 (tick), place white on e4 (tick)
"e7 Be5. g1 Wf3."  → two moves in sequence
```

- Squares: `e2`, `a1`, `h8`. Optional `W`/`B` color prefix required when placing on an empty square.
- Period `.` flushes the current group and queues a tick.

## Provisioning

On boot the board enters BLE advertising mode with the name "ChessBoard". The iOS companion app connects via BLE, sends WiFi credentials and Lichess API token each session (nothing persisted to NVS), and starts games. Only sensor calibration data is persisted to NVS (in the separate `cal` partition).

`just erase-nvs` clears the main NVS partition. It does not affect sensor calibration.

The `IDF_PATH` build variable can still be set in `.env` (loaded via the justfile's `set dotenv-load`).

See `docs/specs/2026-03-31-ble-companion-app-design.md` for the full design.

## Sensor Calibration

Per-board sensor calibration (baseline voltage, detection threshold) is stored in NVS. The diagnostics binary (`src/bin/diagnostics.rs`, flashed via `just flash-diag`) runs a 3-phase pipeline: assembly check (LED sweep → empty board scan → starting position scan), calibration (derives threshold from measured noise floor and weakest piece signal), and change-based diagnosis (logs sensor changes to identify noisy squares).

The production firmware loads calibration from NVS on boot, falling back to `SensorConfig::default()` if uncalibrated.

Calibration data lives in a separate `cal` NVS partition from the main `nvs` partition. This means `just erase-nvs` does not wipe calibration. Use `just erase-cal` to force recalibration.

See `docs/specs/2026-03-28-sensor-diagnostics-design.md` for the full design.

## Coding Conventions

- Error types: use `thiserror` with proper enums (never `()` as error type)
- No `unwrap()` in production code; `expect()` only where failure is logically impossible
- Prefer iterators over index-based loops, borrowing over cloning
- All public types implement `Debug` at minimum
