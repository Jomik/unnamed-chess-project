# Smart Chess Board

The Smart Chess Board is an ESP32-based physical chess board that detects piece positions using Hall-effect sensors and provides visual LED feedback. It integrates the shakmaty chess engine to validate moves and manage game state, enabling a seamless bridge between physical and digital chess.

## Features

- **Piece Detection**: Per-color piece detection using analog hall-effect sensors in an 8x8 grid.
- **Legal Move Validation**: Real-time validation of physical chess moves using the `shakmaty` library.
- **Visual LED Feedback**: Indicators for move destinations, captures, and check alerts.
- **Interactive Simulator**: A terminal-based simulator for manual testing without physical hardware.

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
