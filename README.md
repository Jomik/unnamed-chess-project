# Smart Chess Board

The Smart Chess Board is an ESP32-S3-based physical chess board that detects piece positions using Hall-effect sensors and provides visual LED feedback. It integrates the shakmaty chess engine to validate moves and manage game state, enabling a seamless bridge between physical and digital chess.

## Features

- **Piece Detection**: Per-color piece detection using analog hall-effect sensors in an 8x8 grid.
- **Legal Move Validation**: Real-time validation of physical chess moves using the `shakmaty` library.
- **Visual LED Feedback**: Indicators for move destinations, captures, and check alerts.


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

# Set up rust-analyzer for host-side editor analysis
cp rust-analyzer.toml.example rust-analyzer.toml
sed -i'' -e "s/SET_YOUR_HOST_TARGET_HERE/$(rustc -vV | grep host | cut -d' ' -f2)/" rust-analyzer.toml
```

## Building and Testing

### Host (Development and Testing)

```bash
just test    # Run all tests
```

### ESP32-S3 Firmware

The ESP toolchain is required (`cargo +esp`). The `just` tasks handle
target selection and toolchain flags automatically.

```bash
just build       # Build firmware
just flash       # Flash to device and monitor serial output
just flash-diag  # Flash diagnostics/calibration binary
just erase-nvs   # Erase NVS partition (triggers reprovisioning)
just erase-cal   # Erase sensor calibration (triggers recalibration)
```

Run `just` with no arguments to see all available tasks.

## Companion App

The iOS companion app connects to the board over BLE to configure players and start games. It lives in `companion/ChessBoard/`.

### Prerequisites

Requires Xcode 15+ and [XcodeGen](https://github.com/yonaskolb/XcodeGen):

```bash
brew install xcodegen
```

For physical device deployment, copy `companion/ChessBoard/Local.xcconfig.template` to `Local.xcconfig` and set your `DEVELOPMENT_TEAM` ID. BLE does not work in the simulator — use a physical device for end-to-end testing.

### Building

```bash
just companion-build   # Generate .xcodeproj + build for simulator
just companion-test    # Generate .xcodeproj + run tests
just companion-open    # Generate .xcodeproj + open in Xcode
```

### Neovim (optional)

SourceKit-LSP needs [xcode-build-server](https://github.com/SolaWing/xcode-build-server) to resolve types in XcodeGen projects:

```bash
brew install xcode-build-server
just companion-build
cd companion/ChessBoard && xcode-build-server config -project ChessBoard.xcodeproj -scheme ChessBoard
```

## Board Setup

On first boot, the board advertises over BLE as **ChessBoard**. Use the companion app to connect, configure players, and start a game.

### Sensor Calibration

For reliable piece detection, run the diagnostics binary to calibrate per-board sensor thresholds:

```bash
just flash-diag   # Flash diagnostics binary and monitor serial
```

The diagnostics binary walks through three phases: assembly check (LED sweep → empty board scan → starting position scan), calibration (derives threshold from noise floor and weakest piece signal), and change-based diagnosis (logs sensor changes to identify noisy squares). Calibration data is stored in a separate NVS partition and survives `just erase-nvs`. Use `just erase-cal` to force recalibration.

Without calibration, the firmware falls back to conservative defaults that may not work well for all boards.

### Lichess Integration

Lichess AI and other externally-managed opponents are available via the companion app. The board uses `RemotePlayer` to receive moves from an external source (the companion app or your own controller). This architecture allows the companion app to handle WiFi, API integration, and opponent management while the board focuses on piece detection and move validation.

## Development

You can define scripted sensor states for automated tests using the BoardScript format (see `src/testutil/script.rs`).
