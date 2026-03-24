set dotenv-load

host_target := `rustc -vV | grep host | cut -d' ' -f2`
esp_target := "xtensa-esp32s3-espidf"

# Run development simulator on host
dev:
    cargo run --target {{host_target}} --bin unnamed-chess-project

# Run tests on host
test *args:
    cargo test --target {{host_target}} {{args}}

# Build for ESP32 (uses esp toolchain)
build:
    cargo +esp build --release --target {{esp_target}}

# Flash to ESP32 and monitor serial output
flash:
    cargo +esp espflash flash --release --target {{esp_target}} --bin unnamed-chess-project --monitor

# Flash diagnostics binary and monitor serial output
flash-diag:
    cargo +esp espflash flash --release --target {{esp_target}} --bin diagnostics --monitor

# Monitor ESP32 serial output
monitor:
    cargo +esp espflash monitor

# Install ESP Xtensa toolchain (run once after installing rustup)
setup-esp:
    espup install

# Clone ESP-IDF (run once, required for firmware builds)
setup-idf:
    scripts/setup-idf
