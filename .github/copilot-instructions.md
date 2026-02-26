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
    sensor.rs      — Esp32PieceSensor: reads DRV5055A3QDBZR sensors via ADC + mux (TODO: not yet implemented)
    display.rs     — Esp32LedDisplay: drives WS2812 LEDs for board feedback (TODO: not yet implemented)
  mock/            — Development/test implementation (compiled when NOT espidf)
    mod.rs
    script.rs      — ScriptedSensor: BoardScript-driven mock sensor for tests
    terminal.rs    — Interactive terminal simulator for manual testing
    display.rs     — TerminalDisplay: ANSI terminal BoardDisplay for development
build.rs           — Runs embuild ESP-IDF setup only when targeting espidf
.cargo/config.toml — Cargo config: default ESP32 target, linker, runner, env
sdkconfig.defaults — ESP-IDF kernel config (stack size, FreeRTOS tick rate)
.mise.toml         — Tool versions (rust, espup, cargo-espflash, ldproxy) and task shortcuts
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
      ▼  ByColor<Bitboard> (white and black bitboards)
GameEngine::tick(positions.white | positions.black)
      │
      ├─ process_moves()  → advances Chess position when a legal move is detected
      │
      └─ returns GameState (implements FeedbackSource)
              │
              ▼
        compute_feedback(&state)  → BoardFeedback (per-square LED instructions)
              │
              ▼
        BoardDisplay::show(&feedback)  → LEDs / terminal output
```

- **`ByColor<Bitboard>`** (from `shakmaty`) — pair of 64-bit bitboards, one per color (`.white`, `.black`). Both sensors return this type; `GameEngine::tick()` receives the combined `positions.white | positions.black`.
- **`Chess`** (from `shakmaty`) — Maintains logical game state: piece types, turn, castling rights, en passant.
- **`PieceSensor`** — Trait abstracting sensor input. Implemented by `Esp32PieceSensor` (hardware) and `ScriptedSensor` (mock). Lives in `src/lib.rs`.
- **`GameEngine`** — Bridges physical sensor readings (`Bitboard`) and logical chess state (`Chess`). Lives in `src/game_logic.rs`.
- **`BoardFeedback`** — Maps squares to `SquareFeedback` variants (Origin, Destination, Capture, Check, Checker). Consumed by `BoardDisplay` implementations.
- **`FeedbackSource`** — Trait that `GameState` implements; decouples feedback logic from the engine.
- **`BoardDisplay`** — Trait abstracting visual output. Implemented by `Esp32LedDisplay` (hardware LEDs) and `TerminalDisplay` (ANSI terminal). Mirrors `PieceSensor` on the input side. Lives in `src/lib.rs`.

### Important game engine constraints

- Promotions are **always to Queen** — no piece-selection mechanism exists on the hardware.
- A move is executed when pieces are placed (not just lifted), except en passant (2 pieces lifted triggers processing).
- Illegal physical states are silently ignored; the engine waits for a valid position.

---

## Hardware Architecture

- **MCU**: ESP32 (Xtensa LX6, `xtensa-esp32-espidf` target)
- **Sensors**: 64× TI DRV5055A3QDBZR analog ratiometric Hall-effect sensors arranged in an 8×8 grid
  - Scanned via analog multiplexers and the ESP32 ADC
  - Output > VCC/2 = south pole (white piece), output < VCC/2 = north pole (black piece), ≈ VCC/2 = empty
  - This ADC threshold comparison is what enables per-color detection
- **Current status**: `Esp32PieceSensor::from()` and `read_positions()` are stubbed with `todo!()` — ADC and multiplexer wiring not yet implemented

---

## BoardScript Format (for tests and mock terminal)

`ScriptedSensor` in `src/mock/script.rs` accepts a mini-language for driving sensor state:

- Squares are two characters: `e2`, `a1`, `h8`
- An optional `W` or `B` prefix specifies piece color: `We4`, `Be5`
  - Color prefix is **required** when placing a piece on an empty square
  - Omitting it on an occupied square infers the color from current state (for lifting)
- Squares in the same group are toggled atomically (same tick)
- A period `.` flushes the current group and queues a tick
- `drain()` and `tick()` now return `Result` — `ParseError::MissingColor` fires if a piece is placed without a color prefix

Examples:
```
"e2 We4."     → lift e2, place white on e4, then tick
"e2.  We4."   → lift e2, tick, place white on e4, tick
"e7 Be5. g1 Wf3."  → two moves in sequence
```

`ScriptedSensor` construction also changed:
- `ScriptedSensor::from_bitboards(white, black)` returns `Result` (errors on overlapping squares)
- `load_bitboards(white, black)` replaces the old `load_bitboard`

This format is used extensively in `src/game_logic.rs` tests via `execute_script()`.

---

## Building and Testing

### Host (development / CI tests)

The default cargo target is ESP32 (`xtensa-esp32-espidf`), so host commands
require an explicit `--target` flag. The mise tasks handle this automatically:

```bash
mise run test    # Run all tests on host
mise run dev     # Run interactive terminal simulator on host
```

### ESP32 firmware

> Requires the ESP toolchain. Run `mise install && mise run setup-esp` once per machine.

```bash
mise run build     # Build firmware
mise run flash     # Flash to device
mise run monitor   # Monitor serial output
```

### Linting / formatting

```bash
cargo fmt --all -- --check
cargo +esp clippy --all-targets --all-features --workspace -- -D warnings
```

---

## CI Pipeline (`.github/workflows/rust_ci.yml`)

CI runs two job groups. ESP checks wait for host checks to pass first.

### Host Checks (`dtolnay/rust-toolchain@stable`)

| Step | Command |
|---|---|
| Format check | `cargo fmt --all -- --check --color always` |
| Clippy (host) | `cargo clippy --all-targets --all-features --workspace --target x86_64-unknown-linux-gnu -- -D warnings` |
| Host build | `cargo build --target x86_64-unknown-linux-gnu` |
| Host tests | `cargo test --target x86_64-unknown-linux-gnu` |

### ESP32 Checks (`esp-rs/xtensa-toolchain@v1.6`)

| Step | Command |
|---|---|
| ESP32 build | `cargo +esp build --release --target xtensa-esp32-espidf` |
| Clippy (ESP32) | `cargo +esp clippy --all-targets --all-features --workspace -- -D warnings` |
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

1. **Running `cargo test` without `--target`** defaults to the ESP32 target and will fail without the ESP toolchain. Use `mise run test` or pass `--target <your-host-triple>` explicitly.
2. **Clippy is strict**: `-D warnings` means any clippy warning fails CI. Run clippy locally before pushing.
3. **The `esp32` module does not compile on host** and vice versa for `mock`. Don't mix imports across the boundary.
4. **`Esp32PieceSensor`** methods panic with `todo!()` — this is intentional (ADC/multiplexer hardware not yet wired). Do not remove the `todo!` without implementing that logic.
5. **Promotions are forced to Queen** — when implementing move handling, always filter out non-Queen promotions (see `game_logic.rs:process_moves`).
