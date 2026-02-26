# Smart Chess Board

The Smart Chess Board is an ESP32-based physical chess board that detects piece positions using Hall-effect sensors and provides visual LED feedback. It integrates the shakmaty chess engine to validate moves and manage game state, enabling a seamless bridge between physical and digital chess.

## Features

- **Piece Detection**: Per-color piece detection using Hall-effect sensors (DRV5055A3QDBZR) in an 8x8 grid.
- **Legal Move Validation**: Real-time validation of physical chess moves using the `shakmaty` library.
- **Visual LED Feedback**: Indicators for move destinations, captures, and check alerts.
- **Interactive Simulator**: A terminal-based simulator for manual testing without physical hardware.

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
rust-toolchain.toml — Pins stable Rust toolchain with rustfmt/clippy/rust-analyzer
.mise.toml         — Tool versions (rust, espup, cargo-espflash, ldproxy) and task shortcuts
Cargo.toml         — Dependencies: shakmaty (chess), thiserror (errors), esp-idf-svc (ESP32)
```

## Getting Started

```bash
# Install tooling and ESP targets
mise install && mise run setup-esp

# Set up rust-analyzer for host-side editor analysis
cp rust-analyzer.toml.example rust-analyzer.toml
sed -i'' -e "s/SET_YOUR_HOST_TARGET_HERE/$(rustc -vV | grep host | cut -d' ' -f2)/" rust-analyzer.toml
```

## Building and Testing

### Host (Development and Testing)

```bash
mise run test    # Run all tests
mise run dev     # Run interactive terminal simulator
```

Tests and the simulator require a `--target` flag pointing to your host triple.
The mise tasks handle this automatically via `$HOST_TARGET`.

### ESP32 Firmware

The default cargo target is `xtensa-esp32-espidf`, so `cargo build`
targets the ESP32. The ESP toolchain is required (`cargo +esp`).

```bash
mise run build   # Build firmware
mise run flash   # Flash to device
```

## Development

The project includes an interactive terminal simulator for manual development testing. You can also define scripted sensor states for automated tests using the BoardScript format.

For detailed documentation on internals and coding conventions, refer to `.github/copilot-instructions.md`.
