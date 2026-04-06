set dotenv-load

host_target := `rustc -vV | grep host | cut -d' ' -f2`
esp_target := "xtensa-esp32s3-espidf"

# Run tests on host
test *args:
    cargo test --target {{host_target}} {{args}}

# Format code
fmt:
    cargo fmt --all

# Run clippy on host
clippy:
    cargo clippy --all-targets --all-features --workspace --target {{host_target}} -- -D warnings

# Run all checks (fmt, clippy, test)
check:
    just fmt
    just clippy
    just test

# Run clippy for ESP32 (uses esp toolchain)
clippy-esp:
    cargo +esp clippy --target {{esp_target}} -- -D warnings

# Run all checks including ESP32 clippy (fmt, clippy, test, clippy-esp)
check-all:
    just check
    just clippy-esp

# Build for ESP32 (uses esp toolchain)
build:
    cargo +esp build --release --target {{esp_target}}

# Build diagnostics binary for ESP32 (uses esp toolchain)
build-diag:
    cargo +esp build --release --target {{esp_target}} --bin diagnostics

# Flash to ESP32 and monitor serial output
flash:
    cargo +esp espflash flash --release --target {{esp_target}} --bin unnamed-chess-project --monitor

# Flash diagnostics binary and monitor serial output
flash-diag:
    cargo +esp espflash flash --release --target {{esp_target}} --bin diagnostics --monitor

# Monitor ESP32 serial output
monitor:
    cargo +esp espflash monitor

# Erase NVS partition to force re-provisioning on next boot
erase-nvs:
    cargo +esp espflash erase-parts --partition-table partitions.csv nvs

# Erase calibration partition to force recalibration on next diagnostics run
erase-cal:
    cargo +esp espflash erase-parts --partition-table partitions.csv cal

# Build companion iOS app (simulator)
[working-directory: 'companion/ChessBoard']
companion-build:
    xcodegen generate
    xcodebuild build -project ChessBoard.xcodeproj -scheme ChessBoard -destination 'generic/platform=iOS Simulator' -quiet

# Run companion iOS app tests
[working-directory: 'companion/ChessBoard']
companion-test:
    xcodegen generate
    xcodebuild test -project ChessBoard.xcodeproj -scheme ChessBoard -destination 'platform=iOS Simulator,name=iPhone 17,OS=26.2' -quiet

# Open companion iOS app in Xcode (for device deployment)
[working-directory: 'companion/ChessBoard']
companion-open:
    xcodegen generate
    open ChessBoard.xcodeproj

# Format Swift code (swift-format)
[working-directory: 'companion/ChessBoard']
companion-fmt:
    swift-format format --in-place --recursive .

# Check Swift formatting (strict, errors on violations)
[working-directory: 'companion/ChessBoard']
companion-lint:
    swift-format lint --strict --recursive .

# Install ESP Xtensa toolchain (run once after installing rustup)
setup-esp:
    espup install

# Clone ESP-IDF (run once, required for firmware builds)
setup-idf:
    scripts/setup-idf
