# Provisioning Plan C: Boot Flow Integration + Cleanup

> **For agentic workers:** Use the `implementing` skill to execute this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite `main.rs` to load config from NVS instead of `env!()`, branch into provisioning or normal mode, and clean up `.env.example`, justfile, and `CLAUDE.md`.

**Architecture:** `main()` opens NVS, calls `BoardConfig::load()`, and branches: `None` → `run_provisioning_server()`, `Some(config)` → existing game flow with config values. All `env!()`/`option_env!()` calls are removed.

**Tech Stack:** Rust, `esp-idf-svc`

**Spec:** `docs/specs/2026-03-26-softap-provisioning-design.md`

**Depends on:** Plan A (BoardConfig + validation) and Plan B (NVS load/save + provisioning server)

---

### Task 1: Rewrite `main.rs` boot flow

**Files:**
- Modify: `src/main.rs`

#### Context for implementer

Current `main.rs` flow:
1. Init peripherals, sys_loop, NVS partition
2. Init LED display, show pending status
3. `WifiConnection::connect(modem, sys_loop, nvs, env!("WIFI_SSID"), env!("WIFI_PASSWORD"))`
4. Init sensor
5. Wait for starting position
6. Choose opponent via `option_env!("LICHESS_API_TOKEN")` match
7. Game loop

New flow per spec:
1. Init peripherals, sys_loop, NVS partition
2. Init LED display
3. Clone NVS partition, open `EspNvs` on `"config"` namespace
4. `BoardConfig::load(&nvs)` →
   - `None` or `Err` → `run_provisioning_server(display, modem, sys_loop, partition, nvs)`
   - `Some(config)` → continue to normal mode
5. Normal mode: connect WiFi with `config.wifi_ssid`/`config.wifi_pass`, init sensor, wait for starting position, choose opponent from `config`, game loop

Key changes:
- `NVS partition` is cloned before constructing `EspNvs` (partition implements `Clone` via `Arc`)
- The `EspNvs` is dropped (or moved into provisioning) before `WifiConnection::connect` gets the original partition
- All `env!()` and `option_env!()` calls removed
- `Esp32LichessClient::new(token)` now receives `config.lichess_token.unwrap()` (a `String`, not `&'static str`)
- `LichessConfig` uses `config.lichess_level` with hardcoded clock defaults (10800, 180)

- [ ] **Step 1: Rewrite main.rs**

Replace the entire ESP32 `main()` function. The new version:

```rust
#[cfg(target_os = "espidf")]
fn main() {
    use esp_idf_svc::eventloop::EspSystemEventLoop;
    use esp_idf_svc::hal::adc::oneshot::AdcDriver;
    use esp_idf_svc::hal::delay::FreeRtos;
    use esp_idf_svc::hal::peripherals::Peripherals;
    use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
    use unnamed_chess_project::esp32::config::{LedPalette, SensorConfig};
    use unnamed_chess_project::esp32::{Esp32LedDisplay, Esp32PieceSensor, WifiConnection};
    use unnamed_chess_project::feedback::{BoardFeedback, StatusKind};
    use unnamed_chess_project::player::EmbeddedEngine;
    use unnamed_chess_project::provisioning::BoardConfig;
    use unnamed_chess_project::session::GameSession;
    use unnamed_chess_project::setup::setup_feedback;
    use unnamed_chess_project::{BoardDisplay, PieceSensor};

    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().expect("failed to take peripherals");
    let sys_loop = EspSystemEventLoop::take().expect("failed to take event loop");
    let nvs_partition = EspDefaultNvsPartition::take().expect("failed to take NVS partition");

    let mut display = Esp32LedDisplay::new(peripherals.pins.gpio2, LedPalette::default())
        .expect("failed to init LED display");

    // Load config from NVS
    let nvs = EspNvs::<NvsDefault>::new(nvs_partition.clone(), "config", true)
        .expect("failed to open NVS namespace");

    let config = match BoardConfig::load(&nvs) {
        Ok(Some(config)) => config,
        Ok(None) => {
            log::info!("No config in NVS — entering provisioning mode");
            unnamed_chess_project::esp32::provisioning::run_provisioning_server(
                &mut display,
                peripherals.modem,
                sys_loop,
                nvs_partition,
                nvs,
            );
        }
        Err(e) => {
            log::warn!("NVS read error: {e} — entering provisioning mode");
            unnamed_chess_project::esp32::provisioning::run_provisioning_server(
                &mut display,
                peripherals.modem,
                sys_loop,
                nvs_partition,
                nvs,
            );
        }
    };

    // Drop NVS handle — partition clone is still available for WiFi
    drop(nvs);

    // Normal boot: connect WiFi
    if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Pending)) {
        log::warn!("LED update failed: {e}");
    }
    let _wifi = match WifiConnection::connect(
        peripherals.modem,
        sys_loop,
        nvs_partition,
        &config.wifi_ssid,
        &config.wifi_pass,
    ) {
        Ok(wifi) => {
            log::info!("WiFi connected");
            if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Success)) {
                log::warn!("LED update failed: {e}");
            }
            FreeRtos::delay_ms(500);
            Some(wifi)
        }
        Err(e) => {
            log::warn!("WiFi failed: {e} — continuing without network");
            if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Failure)) {
                log::warn!("LED update failed: {e}");
            }
            FreeRtos::delay_ms(500);
            None
        }
    };
    if let Err(e) = display.show(&BoardFeedback::default()) {
        log::warn!("LED update failed: {e}");
    }

    // Init sensor
    let adc_driver = AdcDriver::new(peripherals.adc1).expect("failed to init ADC1");
    let mut sensor = Esp32PieceSensor::new(
        &adc_driver,
        peripherals.pins.gpio4,
        peripherals.pins.gpio5,
        peripherals.pins.gpio6,
        peripherals.pins.gpio7,
        peripherals.pins.gpio9,
        peripherals.pins.gpio10,
        peripherals.pins.gpio11,
        peripherals.pins.gpio12,
        SensorConfig {
            baseline_mv: 1440,
            threshold_mv: 200,
            settle_delay_ms: 2,
        },
    )
    .expect("failed to init sensor");

    // Wait for starting position
    log::info!("Waiting for starting position...");
    loop {
        let positions = match sensor.read_positions() {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Sensor read failed: {e}");
                FreeRtos::delay_ms(100);
                continue;
            }
        };
        match setup_feedback(&positions) {
            Some(fb) => {
                if let Err(e) = display.show(&fb) {
                    log::warn!("LED update failed: {e}");
                }
            }
            None => break,
        }
        FreeRtos::delay_ms(50);
    }
    log::info!("Starting position detected");
    if let Err(e) = display.show(&BoardFeedback::default()) {
        log::warn!("LED clear failed: {e}");
    }

    // Choose opponent
    let opponent: Box<dyn unnamed_chess_project::player::Player> =
        match config.lichess_token {
            Some(token) if _wifi.is_some() => {
                use unnamed_chess_project::esp32::Esp32LichessClient;
                use unnamed_chess_project::lichess::{LichessConfig, spawn_lichess_opponent};

                let lichess_config = LichessConfig {
                    level: config.lichess_level,
                    clock_limit: 10800,
                    clock_increment: 180,
                };

                let client = Esp32LichessClient::new(token);

                let spawn_fn = |f: Box<dyn FnOnce() + Send>| -> Result<(), String> {
                    std::thread::Builder::new()
                        .stack_size(8192)
                        .spawn(f)
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                };

                match spawn_lichess_opponent(client, lichess_config, spawn_fn) {
                    Ok(lichess_opponent) => {
                        log::info!("Lichess opponent ready");
                        if let Err(e) =
                            display.show(&BoardFeedback::with_status(StatusKind::Success))
                        {
                            log::warn!("LED update failed: {e}");
                        }
                        FreeRtos::delay_ms(500);
                        Box::new(lichess_opponent)
                    }
                    Err(e) => {
                        log::warn!("Lichess setup failed: {e} — falling back to embedded AI");
                        if let Err(e) =
                            display.show(&BoardFeedback::with_status(StatusKind::Failure))
                        {
                            log::warn!("LED update failed: {e}");
                        }
                        FreeRtos::delay_ms(500);
                        Box::new(EmbeddedEngine::new(unsafe {
                            esp_idf_svc::sys::esp_random()
                        }))
                    }
                }
            }
            _ => {
                log::info!("No Lichess token — using embedded AI");
                Box::new(EmbeddedEngine::new(unsafe {
                    esp_idf_svc::sys::esp_random()
                }))
            }
        };

    // Game loop (unchanged)
    use unnamed_chess_project::player::HumanPlayer;

    let initial_positions = match sensor.read_positions() {
        Ok(p) => p,
        Err(e) => {
            log::error!("Initial sensor read failed: {e}");
            return;
        }
    };
    let mut session = GameSession::new(Box::new(HumanPlayer::new(initial_positions)), opponent);
    let mut prev = initial_positions;
    log::info!("Game loop started");

    loop {
        let positions = match sensor.read_positions() {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Sensor read failed: {e}");
                FreeRtos::delay_ms(100);
                continue;
            }
        };

        let white_added = positions.white & !prev.white;
        let white_removed = prev.white & !positions.white;
        let black_added = positions.black & !prev.black;
        let black_removed = prev.black & !positions.black;

        for sq in white_added {
            log::debug!("+ {sq} white");
        }
        for sq in white_removed {
            log::debug!("- {sq} white");
        }
        for sq in black_added {
            log::debug!("+ {sq} black");
        }
        for sq in black_removed {
            log::debug!("- {sq} black");
        }
        prev = positions;

        let result = session.tick(positions);

        if let Some(mv) = &result.last_move {
            log::info!("Move played: {mv}");
        }

        if let Err(e) = display.show(&result.feedback) {
            log::warn!("LED update failed: {e}");
        }

        FreeRtos::delay_ms(50);
    }
}
```

- [ ] **Step 2: Verify ESP32 build**

Run: `just build`
Expected: compiles successfully. No `env!()` or `option_env!()` calls remain.

- [ ] **Step 3: Commit**

Message: `feat: rewrite main.rs to use NVS config with provisioning fallback`

---

### Task 2: Clean up `.env.example` and `.env`

**Files:**
- Modify: `.env.example`
- Modify: `.env` (if it exists)

#### Context for implementer

Remove all credential/config env vars. Keep only `IDF_PATH` (build toolchain config).

- [ ] **Step 1: Update `.env.example`**

Replace contents with:

```
# Pre-cloned ESP-IDF path (workaround for macOS memory issue with recursive git clone)
# If embuild's clone fails on your machine, run `just setup-idf` and uncomment this:
# IDF_PATH=.embuild/espressif/esp-idf/v5.5.3
```

- [ ] **Step 2: Update `.env` (if it exists)**

Remove `WIFI_SSID`, `WIFI_PASSWORD`, `LICHESS_API_TOKEN`, `LICHESS_AI_LEVEL`, `LICHESS_CLOCK_LIMIT`, `LICHESS_CLOCK_INCREMENT`. Keep `IDF_PATH` if set.

- [ ] **Step 3: Commit**

Message: `chore: remove credential env vars from .env.example`

---

### Task 3: Add `erase-nvs` justfile recipe

**Files:**
- Modify: `justfile`

#### Context for implementer

The NVS partition starts at offset `0x9000` with size `0x6000` (per ESP-IDF defaults for the partition layout in `partitions.csv`). The `erase-nvs` recipe calls `esptool.py erase_region`.

Also consider whether `set dotenv-load` is still needed — it is, if users still use `IDF_PATH` in `.env`.

- [ ] **Step 1: Add erase-nvs recipe to justfile**

Add after the existing `monitor` recipe:

```just
# Erase NVS partition to force re-provisioning on next boot
erase-nvs:
    esptool.py erase_region 0x9000 0x6000
```

- [ ] **Step 2: Commit**

Message: `feat: add just erase-nvs recipe for re-provisioning`

---

### Task 4: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

#### Context for implementer

Update the "Environment Variables" section to describe the new provisioning flow. Remove references to compile-time `env!()` / `option_env!()` for credentials.

- [ ] **Step 1: Replace the Environment Variables section**

Replace the current "## Environment Variables" section (line 91-93) with:

```markdown
## Provisioning

Runtime configuration (WiFi credentials, Lichess settings) is stored in the ESP32's NVS partition, not in environment variables. On first boot (or after `just erase-nvs`), the board enters SoftAP provisioning mode — connect to the `ChessBoard` WiFi network and navigate to `192.168.4.1` to configure.

The `IDF_PATH` build variable can still be set in `.env` (loaded via the justfile's `set dotenv-load`).

See `docs/specs/2026-03-26-softap-provisioning-design.md` for the full design.
```

- [ ] **Step 2: Run `just check`**

Expected: all checks pass (no code changes in this task, but verify nothing broke)

- [ ] **Step 3: Commit**

Message: `docs: update CLAUDE.md for provisioning flow`
