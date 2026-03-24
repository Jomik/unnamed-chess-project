# Smart Chess Board

The Smart Chess Board is an ESP32-S3-based physical chess board that detects piece positions using Hall-effect sensors and provides visual LED feedback. It integrates the shakmaty chess engine to validate moves and manage game state, enabling a seamless bridge between physical and digital chess.

## Features

- **Piece Detection**: Per-color piece detection using analog hall-effect sensors in an 8x8 grid.
- **Legal Move Validation**: Real-time validation of physical chess moves using the `shakmaty` library.
- **Visual LED Feedback**: Indicators for move destinations, captures, and check alerts.
- **Interactive Simulator**: A terminal-based simulator for manual testing without physical hardware.

## Getting Started

### Prerequisites

Install [Rust](https://rustup.rs), [just](https://github.com/casey/just), and the [ESP toolchain](https://github.com/esp-rs/espup#installation):

```bash
rustup component add rust-src llvm-tools rust-analyzer

cargo install espup cargo-espflash ldproxy
espup install
# Source ~/export-esp.sh in your shell profile (see espup output for details)

# Clone ESP-IDF (required for firmware builds)
just setup-idf

# Configure local environment
cp .env.example .env
# Edit .env with your WiFi credentials

# Set up rust-analyzer for host-side editor analysis
cp rust-analyzer.toml.example rust-analyzer.toml
sed -i'' -e "s/SET_YOUR_HOST_TARGET_HERE/$(rustc -vV | grep host | cut -d' ' -f2)/" rust-analyzer.toml
```

## Building and Testing

### Host (Development and Testing)

```bash
just test    # Run all tests
just dev     # Run interactive terminal simulator
```

### ESP32-S3 Firmware

The ESP toolchain is required (`cargo +esp`). The `just` tasks handle
target selection and toolchain flags automatically.

```bash
just build   # Build firmware
just flash   # Flash to device and monitor serial output
```

Run `just` with no arguments to see all available tasks.

## Development

The project includes an interactive terminal simulator for manual development testing. You can also define scripted sensor states for automated tests using the BoardScript format.
