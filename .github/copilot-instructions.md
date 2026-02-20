# Copilot Instructions — Smart Chess Board

## Project Overview

This is an ESP32-based smart chess board that uses Hall-effect sensors to detect piece positions on a physical chess board and provides visual LED feedback to guide players.

**Target audience for this file**: A coding agent or contributor seeing this codebase for the first time.

---

## Repository Layout

```
src/
  main.rs          — Binary entry point (conditional: ESP32 or host mock)
  lib.rs           — Library root with conditional module registration
  game_logic.rs    — Core GameEngine: processes sensor bitboards, advances chess position
  feedback.rs      — BoardFeedback: maps board state → per-square LED instructions
  esp32/           — Hardware implementation (only compiled for target_os = "espidf")
    mod.rs
    sensor.rs      — Esp32PieceSensor: reads 74HC165 shift registers (TODO: GPIO not yet implemented)
  mock/            — Development/test implementation (compiled when NOT espidf)
    mod.rs
    script.rs      — ScriptedSensor: BoardScript-driven mock sensor for tests
    terminal.rs    — Interactive terminal simulator for manual testing
build.rs           — Runs embuild ESP-IDF setup only when targeting espidf
.cargo/config.toml — Linker, runner, and env for ESP32 target
sdkconfig.defaults — ESP-IDF kernel config (stack size, FreeRTOS tick rate)
rust-toolchain.toml — Pins stable Rust toolchain with rustfmt/clippy/rust-analyzer
.mise.toml         — Tool versions (rust, espup, cargo-espflash, ldproxy) and task shortcuts
Cargo.toml         — Dependencies: shakmaty (chess), thiserror (errors), esp-idf-svc (ESP32)
```

---

## Conditional Compilation

Two mutually exclusive code paths exist based on the compilation target:

| Context | Target | Active modules |
|---|---|---|
| ESP32 firmware | `xtensa-esp32-espidf` (`target_os = "espidf"`) | `esp32::*` |
| Host (dev/test) | `x86_64-unknown-linux-gnu` | `mock::*` |

Use `#[cfg(target_os = "espidf")]` / `#[cfg(not(target_os = "espidf"))]` for target-specific code. Do **not** put hardware-specific imports in common modules.

---

## Key Types and Data Flow

```
Physical sensors
      │
      ▼  Bitboard (u64, one bit per square, all colors combined)
GameEngine::tick(current_bb)
      │
      ├─ process_moves()  → advances Chess position when a legal move is detected
      │
      └─ returns GameState (implements FeedbackSource)
              │
              ▼
        compute_feedback(&state)  → BoardFeedback (per-square LED instructions)
```

- **`Bitboard`** (from `shakmaty`) — 64-bit set of occupied squares. The sensor returns **one combined bitboard** for all pieces (not per-color).
- **`Chess`** (from `shakmaty`) — Maintains logical game state: piece types, turn, castling rights, en passant.
- **`GameEngine`** — Bridges physical sensor readings (`Bitboard`) and logical chess state (`Chess`). Lives in `src/game_logic.rs`.
- **`BoardFeedback`** — Maps squares to `SquareFeedback` variants (Origin, Destination, Capture, Check, Checker). Consumed by LED drivers or terminal rendering.
- **`FeedbackSource`** — Trait that `GameState` implements; decouples feedback logic from the engine.

### Important game engine constraints

- Promotions are **always to Queen** — no piece-selection mechanism exists on the hardware.
- A move is executed when pieces are placed (not just lifted), except en passant (2 pieces lifted triggers processing).
- Illegal physical states are silently ignored; the engine waits for a valid position.

---

## Hardware Architecture

- **MCU**: ESP32 (Xtensa LX6, `xtensa-esp32-espidf` target)
- **Sensors**: 64× TI DRV5032FB digital Hall-effect sensors arranged in an 8×8 grid
  - Output is active LOW: LOW = magnet (piece) present, HIGH = empty
  - 8 sensors per 74HC165 8-bit parallel-in/serial-out shift register
- **Shift registers**: 8× 74HC165 daisy-chained; 3 GPIO pins required (CLK, PL/LATCH, Q7/DATA)
- **Current status**: `Esp32PieceSensor::from()` and `read_positions()` are stubbed with `todo!()` — GPIO wiring not yet implemented

---

## BoardScript Format (for tests and mock terminal)

`ScriptedSensor` in `src/mock/script.rs` accepts a mini-language for driving sensor state:

- Squares are two characters: `e2`, `a1`, `h8`
- Squares in the same group are toggled atomically (same tick)
- A period `.` flushes the current group and queues a tick

Examples:
```
"e2e4."     → toggle e2 & e4 together, then tick
"e2.  e4."  → toggle e2, tick, toggle e4, tick
"e2. e4."   → same (spaces ignored within a group)
```

This format is used extensively in `src/game_logic.rs` tests via `execute_script()`.

---

## Building and Testing

### Host (development / CI tests)

```bash
# Run all tests (fast, no hardware required)
cargo test --target x86_64-unknown-linux-gnu

# Run interactive terminal simulator
cargo run --target x86_64-unknown-linux-gnu

# Or use mise shortcuts
mise run test
mise run dev
```

### ESP32 firmware

> Requires the ESP toolchain. Run `mise install && mise run setup-esp` once per machine.

```bash
# Build firmware
cargo +esp build --release --target xtensa-esp32-espidf
mise run build

# Flash and monitor
mise run flash
mise run monitor
```

### Linting / formatting

```bash
cargo fmt --all -- --check
cargo +esp clippy --all-targets --all-features --workspace -- -D warnings
```

---

## CI Pipeline (`.github/workflows/rust_ci.yml`)

The CI matrix runs all of these in parallel:

| Step | Command |
|---|---|
| ESP32 build | `cargo +esp build --release --target xtensa-esp32-espidf` |
| Format check | `cargo fmt --all -- --check --color always` |
| Clippy (ESP32) | `cargo +esp clippy --all-targets --all-features --workspace -- -D warnings` |
| Host build | `cargo build --target x86_64-unknown-linux-gnu` |
| Host tests | `cargo test --target x86_64-unknown-linux-gnu` |

CI uses `esp-rs/xtensa-toolchain@v1.6` and `Swatinem/rust-cache@v2`.

**All CI steps must pass before merging.** Clippy warnings are treated as errors (`-D warnings`).

---

## Coding Conventions

See `.github/instructions/rust.instructions.md` for full Rust conventions. Key points:

- **Error handling**: Use `Result<T, E>` with meaningful error types via `thiserror`. Never use `()` as an error type.
- **No `unwrap()`** in production code — use `?` or `expect()` only where logically impossible to fail, with a clear message.
- **Documentation**: Brief `///` on all public items. Focus on "what" and "why", not tutorials. Hardware-specific details (pins, timing) belong in doc comments.
- **Traits**: Implement `Debug` on all public types at minimum.
- **Formatting**: `cargo fmt` enforced. Lines ≤ 100 characters.
- **Testing**: Unit tests in `#[cfg(test)]` modules alongside source. Use `test_case` for parametric tests.

---

## Common Pitfalls

1. **Running `cargo test` without a target** will attempt an ESP-IDF build and fail outside an ESP environment. Always use `--target x86_64-unknown-linux-gnu` for tests.
2. **Clippy is strict**: `-D warnings` means any clippy warning fails CI. Run clippy locally before pushing.
3. **The `esp32` module does not compile on host** and vice versa for `mock`. Don't mix imports across the boundary.
4. **`Esp32PieceSensor`** methods panic with `todo!()` — this is intentional (hardware not yet wired). Do not remove the `todo!` without implementing the GPIO logic.
5. **Promotions are forced to Queen** — when implementing move handling, always filter out non-Queen promotions (see `game_logic.rs:process_moves`).
