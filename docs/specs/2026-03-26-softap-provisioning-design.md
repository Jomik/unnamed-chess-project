# SoftAP Provisioning Design

**Date:** 2026-03-26
**Status:** Draft

## Motivation

The firmware currently bakes WiFi credentials and Lichess configuration into the binary via compile-time `env!()` / `option_env!()` macros loaded from `.env`. This has three problems:

1. **Distribution:** A pre-built binary cannot be flashed to a board with different WiFi credentials without recompiling.
2. **Reconfiguration:** Changing WiFi password or Lichess settings requires a full rebuild and re-flash.
3. **Security:** Credentials are embedded in the binary, which is problematic if the binary or repository is shared.

## Solution Overview

Replace compile-time environment variables with runtime provisioning via a SoftAP web page. On first boot (or after NVS erase), the board starts a WiFi hotspot and serves an HTML configuration form. The user fills in credentials from any device with a browser, the board saves them to NVS, and reboots into normal mode.

## NVS Config Store

All runtime configuration is stored in the NVS partition (existing 24KB `nvs` partition in `partitions.csv`) under the namespace `"config"`.

### Stored Values

| NVS Key         | Rust Type        | Required | Default | Validation             | NVS Buffer Size |
|-----------------|------------------|----------|---------|------------------------|-----------------|
| `wifi_ssid`     | `String`         | yes      | —       | Non-empty, ≤ 32 bytes | 33 bytes        |
| `wifi_pass`     | `String`         | yes      | —       | Non-empty, ≤ 64 bytes   | 65 bytes        |
| `lichess_token` | `Option<String>` | no       | `None`  | —                      | 128 bytes       |
| `lichess_level` | `u8`             | no       | 2       | 1–8                    | (stored as `u8`)|

> **Behavioral change:** The previous compile-time default for `LICHESS_AI_LEVEL` was `4`. This spec intentionally lowers the default to `2` for a more accessible out-of-box experience.

The `wifi_pass` field is required and must be non-empty. Open (passwordless) networks are not supported — `WifiConnection::connect` hardcodes `AuthMethod::WPA2Personal`, and updating it for open networks is out of scope for this change.

NVS buffer sizes are for `EspNvs::get_str()` calls, which require a caller-provided `&mut [u8]` scratch buffer. Sizes include the NUL terminator. The Lichess token buffer is 128 bytes, which comfortably exceeds current Lichess personal access token lengths (~40 chars).

### `BoardConfig` Struct

```rust
pub struct BoardConfig {
    pub wifi_ssid: String,
    pub wifi_pass: String,
    pub lichess_token: Option<String>,
    pub lichess_level: u8,
}
```

- `BoardConfig::load(nvs: &EspNvs<NvsDefault>) -> Result<Option<BoardConfig>, ProvisioningError>` — reads from NVS via `get_str` into stack-allocated buffers. Returns `Ok(None)` if `wifi_ssid` key is missing (triggers provisioning). Returns `Err` on NVS read failures. Takes a shared reference to `EspNvs` (the esp-idf-svc NVS methods take `&self`), not ownership — the NVS handle and `EspDefaultNvsPartition` remain available for other uses (see Resource Lifecycle below).
- `BoardConfig::save(&self, nvs: &EspNvs<NvsDefault>) -> Result<(), ProvisioningError>` — writes all fields to NVS via `set_str` / `set_u8`. (ESP-IDF NVS handles internal locking, so `&self` is sufficient for writes too.)

Validation logic (SSID length, level range) is extracted into a platform-independent method on `BoardConfig` so it can be unit-tested on the host.

## Boot Flow

### Resource Lifecycle

`EspDefaultNvsPartition` is a singleton obtained via `.take()`. The boot flow must carefully sequence its use:

1. `EspDefaultNvsPartition::take()` — obtain the partition handle once.
2. Construct `EspNvs::<NvsDefault>::new(partition.clone(), "config", true)` — this clones the partition handle (it implements `Clone`), so the original remains available.
3. `BoardConfig::load(&nvs)` — borrows `EspNvs` by shared reference, returns config.
4. Drop `EspNvs` (or keep it alive if provisioning mode needs to write).
5. In normal mode, `partition` is passed to `WifiConnection::connect()` which currently takes `EspDefaultNvsPartition` by value.

Similarly, `peripherals.modem` is a move-once singleton. Provisioning mode and normal mode both need the WiFi modem, but they never coexist in the same boot — provisioning always ends with `esp_restart()`, so normal mode runs in a fresh process where `Peripherals::take()` succeeds again.

```
init peripherals + NVS partition
    → open EspNvs on "config" namespace (cloning partition handle)
    → BoardConfig::load(&nvs)
        → Err(_):  treat as missing config, enter provisioning
        → Ok(None): enter provisioning mode (uses modem for SoftAP, reboots after save)
        → Ok(Some(config)): enter normal mode (uses modem for STA)
```

### Provisioning Mode

1. LEDs show `status_pending` pattern (pulsing blue) to indicate awaiting setup.
2. Start SoftAP with SSID `"ChessBoard"` (open network, no password) — consumes `peripherals.modem`.
3. Start HTTP server on `192.168.71.1:80`.
4. Serve config form on `GET /`, accept submission on `POST /`.
5. On valid submission: save to NVS, show `status_success` LED flash, respond with success page, reboot after short delay via `esp_restart()`.
6. On validation error: re-render form with inline error messages.

After reboot, `Peripherals::take()` and `EspDefaultNvsPartition::take()` succeed again in the new process, and the normal-mode branch runs with the freshly stored config.

### Normal Mode

Existing boot flow, but reading from `BoardConfig` instead of `env!()`:

```
connect WiFi(config.wifi_ssid, config.wifi_pass)
    → wait for starting position
    → choose opponent:
        if config.lichess_token.is_some() && wifi connected:
            LichessConfig { level: config.lichess_level, clock_limit: 10800, clock_increment: 180 }
        else:
            EmbeddedEngine
    → game loop
```

Clock limit (10800) and clock increment (180) remain hardcoded defaults — they were removed from runtime config to keep the form simple.

## SoftAP + HTTP Server

### SoftAP Configuration

- SSID: `"ChessBoard"` (open, no password)
- Default gateway/IP: `192.168.71.1`
- No DNS redirect — user manually navigates to `192.168.71.1`. Note: some devices (iOS, Android) probe known URLs for internet connectivity and may show a "no internet" warning when connected to the SoftAP. This is cosmetic — the form is still accessible at `192.168.71.1`. A DNS redirect (captive portal) can be added as a future enhancement if this proves confusing.

### HTTP Endpoints

**`GET /`** — serves the HTML provisioning form. The HTML is embedded in the binary via `include_str!("provisioning.html")`.

**`POST /`** — receives `application/x-www-form-urlencoded` form data with fields: `wifi_ssid`, `wifi_pass`, `lichess_token`, `lichess_level`. Validates input, writes to NVS on success, returns a success or error page.

### HTML Form

A single minimal HTML page (`src/esp32/provisioning.html`) with:

- WiFi SSID — text input, required
- WiFi Password — password input, required
- Lichess API Token — text input, optional, with a link to the Lichess token creation URL (`https://lichess.org/account/oauth/token/create?scopes[]=board:play&scopes[]=challenge:write&description=Chess+Board`)
- AI Level — number input, 1–8, default 2, optional
- Submit button

Minimal inline CSS for readability. No external dependencies. Server-side rendering of error messages (re-serve the form HTML with error text inserted).

## Re-Provisioning

To change configuration after initial setup:

1. Run `just erase-nvs` (erases the NVS partition via `esptool.py erase_region`).
2. Reboot the board — it will detect missing config and enter provisioning mode.

A normal firmware flash (`just flash`) does **not** erase NVS, so firmware updates preserve existing configuration.

## LED Feedback

| State                        | LED Display                          |
|------------------------------|--------------------------------------|
| Provisioning mode (waiting)  | `status_pending` (pulsing blue)      |
| Config saved successfully    | `status_success` (green flash)       |
| Normal boot WiFi connecting  | Existing `status_pending` behavior   |
| Normal boot WiFi failed      | Existing `status_failure` behavior   |

## Error Handling

- **NVS read error on boot:** Treat as missing config → enter provisioning mode (self-healing).
- **Form validation failure:** Re-render form with inline error message. Do not save partial config.
- **WiFi connection failure with stored config:** Existing behavior — log warning, continue without network, fall back to embedded AI.

## Changes to Existing Code

### `src/esp32/lichess.rs`

`Esp32LichessClient` currently stores the token as `&'static str` because compile-time `env!()` produces a `&'static str`. With NVS-sourced config, the token is an owned `String` with no `'static` lifetime.

Three structs hold the token as `&'static str` and all must be updated:

- `Esp32LichessClient { token: &'static str }` — change to `String`
- `Esp32LichessGame { token: &'static str }` — change to `String`
- `Esp32LichessStreamImpl { post_token: &'static str }` — change to `String`

The token flows via ownership transfer: `Esp32LichessClient::challenge_ai` moves `self.token` into `Esp32LichessGame`, and `into_stream` moves it into `Esp32LichessStreamImpl`. This chain continues to work with `String` — no lifetime gymnastics needed.

`Esp32LichessStreamImpl::connect` parameter also changes from `token: &'static str` to `token: String`.

```rust
pub struct Esp32LichessClient {
    token: String,
}

impl Esp32LichessClient {
    pub fn new(token: String) -> Self { ... }
}
```

This is a signature-breaking change. Call sites in `main.rs` pass the owned `String` directly.

### `main.rs`

- Remove all `env!()` and `option_env!()` calls.
- Add NVS partition + `EspNvs` setup, then `BoardConfig::load()`.
- Branch on config presence: provisioning mode vs. normal mode.
- Pass `config.wifi_ssid` / `config.wifi_pass` to `WifiConnection::connect()`.
- Build `LichessConfig` from `config.lichess_level` with hardcoded clock defaults.
- Pass `config.lichess_token` (owned `String`) to `Esp32LichessClient::new()`.

### `.env.example` / `.env`

Remove:
- `WIFI_SSID`
- `WIFI_PASSWORD`
- `LICHESS_API_TOKEN`
- `LICHESS_AI_LEVEL`
- `LICHESS_CLOCK_LIMIT`
- `LICHESS_CLOCK_INCREMENT`

Keep:
- `IDF_PATH` (build toolchain config, not a runtime credential)

### `justfile`

- Remove dotenv-related config for the removed variables (keep `set dotenv-load` if `IDF_PATH` still needs it).
- Add `erase-nvs` recipe targeting the NVS partition offset.

### `CLAUDE.md`

- Update "Environment Variables" section to describe the new provisioning flow.
- Remove references to compile-time `env!()` / `option_env!()` for credentials.

## New Files

| File                            | Purpose                                                  |
|---------------------------------|----------------------------------------------------------|
| `src/provisioning.rs`           | `BoardConfig` struct, `ValidationError`, `validate()` — platform-independent, host-testable |
| `src/esp32/provisioning.rs`     | `BoardConfig` NVS `load`/`save` impl, `ProvisioningError`, SoftAP + HTTP server |
| `src/esp32/provisioning.html`   | Single-page config form (embedded via `include_str!`)    |

`BoardConfig` and validation live outside `esp32/` so they compile on the host target and can be unit-tested via `just test`. The ESP-IDF-specific methods (`load`, `save`) are added via a `cfg`-gated `impl BoardConfig` block in `src/esp32/provisioning.rs`.

### Module Registration

`src/lib.rs` gains:
```rust
pub mod provisioning;
```

`src/esp32/mod.rs` gains:
```rust
pub mod provisioning;
pub use provisioning::ProvisioningError;
```

## Testing Strategy

### Host-Testable (unit tests)

- `BoardConfig` validation: SSID length, password length, level range, edge cases.

### Hardware-Only (manual testing)

- NVS read/write round-trip.
- SoftAP startup and HTTP form serving.
- Form submission → NVS save → reboot → normal boot with stored config.
- `just erase-nvs` → reboot → provisioning mode re-entry.
- WiFi failure fallback with stored config.

### New Justfile Command

- `just erase-nvs` — erases the NVS partition to force re-provisioning on next boot. The partition offset and size must be derived from `partitions.csv` (the NVS entry has no explicit offset, so it follows the ESP-IDF default layout starting at `0x9000` with size `0x6000`). The recipe calls `esptool.py erase_region 0x9000 0x6000`.
