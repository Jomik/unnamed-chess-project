# Provisioning Plan B: NVS Storage + SoftAP Provisioning Server

> **For agentic workers:** Use the `implementing` skill to execute this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement NVS load/save for `BoardConfig` and the SoftAP HTTP provisioning server that serves a config form, validates input, saves to NVS, and reboots.

**Architecture:** ESP-IDF-specific `impl BoardConfig` methods (`load`, `save`) live in `src/esp32/provisioning.rs`, extending the host-visible struct from `src/provisioning.rs` (Plan A). A `run_provisioning_server` function starts SoftAP + HTTP server and blocks until config is saved. The HTML form is a separate `.html` file embedded via `include_str!`.

**Tech Stack:** Rust, `esp-idf-svc` (NVS, WiFi/SoftAP, HTTP server), `thiserror`

**Spec:** `docs/specs/2026-03-26-softap-provisioning-design.md`

**Depends on:** Plan A (BoardConfig struct + validation must exist)

---

### Task 1: NVS load/save for BoardConfig

**Files:**
- Create: `src/esp32/provisioning.rs`
- Modify: `src/esp32/mod.rs` (add `pub mod provisioning;`)

#### Context for implementer

The spec defines NVS storage under namespace `"config"` with these keys and buffer sizes:

| NVS Key         | Type   | Buffer Size | Default |
|-----------------|--------|-------------|---------|
| `wifi_ssid`     | string | 33 bytes    | —       |
| `wifi_pass`     | string | 65 bytes    | —       |
| `lichess_token` | string | 128 bytes   | None    |
| `lichess_level` | u8     | n/a         | 2       |

API signatures from esp-idf-svc 0.52.1:
- `EspNvs::<NvsDefault>::new(partition.clone(), "config", true)` — `true` enables read-write
- `nvs.get_str(name, &mut buf)` → `Result<Option<&str>, EspError>` (takes `&self`)
- `nvs.set_str(name, val)` → `Result<(), EspError>` (takes `&self`)
- `nvs.get_u8(name)` → `Result<Option<u8>, EspError>`
- `nvs.set_u8(name, val)` → `Result<(), EspError>`
- `EspDefaultNvsPartition` implements `Clone` (wraps `Arc`)

`BoardConfig::load` returns `Ok(None)` when `wifi_ssid` is missing (triggers provisioning mode). `BoardConfig::save` calls `validate()` before writing.

This code is ESP32-only (`#[cfg(target_os = "espidf")]`). It cannot be tested on host — verify with `just build`.

- [ ] **Step 1: Create `src/esp32/provisioning.rs` with error type and NVS constants**

```rust
use esp_idf_svc::nvs::{EspNvs, NvsDefault};
use crate::provisioning::{BoardConfig, ValidationError};

const NVS_NAMESPACE: &str = "config";
const KEY_WIFI_SSID: &str = "wifi_ssid";
const KEY_WIFI_PASS: &str = "wifi_pass";
const KEY_LICHESS_TOKEN: &str = "lichess_tok";
const KEY_LICHESS_LEVEL: &str = "lichess_lvl";

// NVS get_str buffer sizes (including NUL terminator)
const BUF_SSID: usize = 33;
const BUF_PASS: usize = 65;
const BUF_TOKEN: usize = 128;

#[derive(Debug, thiserror::Error)]
pub enum ProvisioningError {
    #[error("NVS error: {0}")]
    Nvs(esp_idf_svc::sys::EspError),
    #[error("validation failed: {0}")]
    Validation(#[from] ValidationError),
}
```

Note: NVS key names are limited to 15 characters, so we use abbreviated keys (`lichess_tok`, `lichess_lvl`).

- [ ] **Step 2: Implement `BoardConfig::load`**

```rust
impl BoardConfig {
    /// Load config from NVS. Returns `Ok(None)` if wifi_ssid is missing
    /// (first boot / after erase). Returns `Err` on NVS read failures.
    pub fn load(nvs: &EspNvs<NvsDefault>) -> Result<Option<Self>, ProvisioningError> {
        let mut ssid_buf = [0u8; BUF_SSID];
        let wifi_ssid = match nvs.get_str(KEY_WIFI_SSID, &mut ssid_buf)
            .map_err(ProvisioningError::Nvs)? {
            Some(s) => s.to_string(),
            None => return Ok(None),
        };

        let mut pass_buf = [0u8; BUF_PASS];
        let wifi_pass = nvs.get_str(KEY_WIFI_PASS, &mut pass_buf)
            .map_err(ProvisioningError::Nvs)?
            .unwrap_or("")
            .to_string();

        let mut token_buf = [0u8; BUF_TOKEN];
        let lichess_token = nvs.get_str(KEY_LICHESS_TOKEN, &mut token_buf)
            .map_err(ProvisioningError::Nvs)?
            .map(|s| s.to_string());

        let lichess_level = nvs.get_u8(KEY_LICHESS_LEVEL)
            .map_err(ProvisioningError::Nvs)?
            .unwrap_or(BoardConfig::DEFAULT_LEVEL);

        Ok(Some(BoardConfig {
            wifi_ssid,
            wifi_pass,
            lichess_token,
            lichess_level,
        }))
    }
}
```

- [ ] **Step 3: Implement `BoardConfig::save`**

```rust
impl BoardConfig {
    /// Validate and save config to NVS.
    pub fn save(&self, nvs: &EspNvs<NvsDefault>) -> Result<(), ProvisioningError> {
        self.validate()?;

        nvs.set_str(KEY_WIFI_SSID, &self.wifi_ssid)
            .map_err(ProvisioningError::Nvs)?;
        nvs.set_str(KEY_WIFI_PASS, &self.wifi_pass)
            .map_err(ProvisioningError::Nvs)?;

        if let Some(ref token) = self.lichess_token {
            nvs.set_str(KEY_LICHESS_TOKEN, token)
                .map_err(ProvisioningError::Nvs)?;
        }

        nvs.set_u8(KEY_LICHESS_LEVEL, self.lichess_level)
            .map_err(ProvisioningError::Nvs)?;

        Ok(())
    }
}
```

- [ ] **Step 4: Register module in `src/esp32/mod.rs`**

Add `pub mod provisioning;` and `pub use provisioning::ProvisioningError;` to `src/esp32/mod.rs`.

Note: The spec says to also add `pub use provisioning::BoardConfig` here. Do **not** add that — `BoardConfig` lives in `crate::provisioning` (from Plan A), not `crate::esp32::provisioning`. Consumers import it as `crate::provisioning::BoardConfig`.

- [ ] **Step 5: Verify ESP32 build**

Run: `just build`
Expected: compiles successfully

- [ ] **Step 6: Commit**

Message: `feat: add NVS load/save for BoardConfig`

---

### Task 2: HTML provisioning form

**Files:**
- Create: `src/esp32/provisioning.html`

#### Context for implementer

A single minimal HTML page served by the provisioning HTTP server. Fields:

- WiFi SSID — text input, required
- WiFi Password — password input, required
- Lichess API Token — text input, optional, with link to `https://lichess.org/account/oauth/token/create?scopes[]=board:play&scopes[]=challenge:write&description=Chess+Board`
- AI Level — number input, 1–8, default 2, optional
- Submit button

Must support server-side error rendering: the form HTML will have a placeholder `<!-- ERRORS -->` that the server replaces with error messages on validation failure.

Minimal inline CSS. No external dependencies (no CDN links — the board has no internet in SoftAP mode).

- [ ] **Step 1: Create the HTML form**

Create `src/esp32/provisioning.html`:

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Chess Board Setup</title>
    <style>
        body { font-family: sans-serif; max-width: 480px; margin: 2em auto; padding: 0 1em; }
        label { display: block; margin-top: 1em; font-weight: bold; }
        input { width: 100%; padding: 0.5em; margin-top: 0.25em; box-sizing: border-box; }
        button { margin-top: 1.5em; padding: 0.75em 2em; font-size: 1em; }
        .error { color: #c00; margin-top: 1em; padding: 0.5em; border: 1px solid #c00; }
        .hint { font-size: 0.85em; color: #666; margin-top: 0.25em; }
        fieldset { border: 1px solid #ccc; padding: 1em; margin-top: 1.5em; }
        legend { font-weight: bold; }
    </style>
</head>
<body>
    <h1>Chess Board Setup</h1>
    <!-- ERRORS -->
    <form method="POST" action="/">
        <fieldset>
            <legend>WiFi</legend>
            <label for="wifi_ssid">Network Name (SSID)</label>
            <input type="text" id="wifi_ssid" name="wifi_ssid" required maxlength="32">

            <label for="wifi_pass">Password</label>
            <input type="password" id="wifi_pass" name="wifi_pass" required maxlength="64">
        </fieldset>

        <fieldset>
            <legend>Lichess (optional)</legend>
            <label for="lichess_token">API Token</label>
            <input type="text" id="lichess_token" name="lichess_token" maxlength="127">
            <p class="hint">
                <a href="https://lichess.org/account/oauth/token/create?scopes[]=board:play&scopes[]=challenge:write&description=Chess+Board">
                    Create a token on lichess.org
                </a>
            </p>

            <label for="lichess_level">AI Level (1–8)</label>
            <input type="number" id="lichess_level" name="lichess_level" min="1" max="8" value="2">
        </fieldset>

        <button type="submit">Save &amp; Connect</button>
    </form>
</body>
</html>
```

- [ ] **Step 2: Commit**

Message: `feat: add provisioning HTML form`

---

### Task 3: SoftAP provisioning server

**Files:**
- Modify: `src/esp32/provisioning.rs` (add `run_provisioning_server`)

#### Context for implementer

`run_provisioning_server` takes the display, modem, sys_loop, nvs partition, and an EspNvs handle. It:

1. Shows `status_pending` LED pattern
2. Starts SoftAP (SSID: `"ChessBoard"`, open, no password)
3. Starts an HTTP server on port 80
4. `GET /` → serves the HTML form (via `include_str!`)
5. `POST /` → parses `application/x-www-form-urlencoded` body, builds `BoardConfig`, validates, saves to NVS, returns success page, triggers reboot
6. On validation error → re-renders form with error message inserted at `<!-- ERRORS -->`
7. Blocks forever (the reboot exits the process)

The HTTP server uses `esp_idf_svc::http::server::EspHttpServer`. Form body parsing is manual — split on `&`, split each pair on `=`, URL-decode values.

SoftAP setup uses `esp_idf_svc::wifi::EspWifi` in AP mode with `AccessPointConfiguration`.

The function signature:

```rust
pub fn run_provisioning_server(
    display: &mut impl crate::BoardDisplay,
    modem: impl esp_idf_svc::hal::modem::WifiModemPeripheral + 'static,
    sys_loop: esp_idf_svc::eventloop::EspSystemEventLoop,
    nvs_partition: esp_idf_svc::nvs::EspDefaultNvsPartition,
    nvs: esp_idf_svc::nvs::EspNvs<NvsDefault>,
) -> !
```

It returns `!` (never) because it always ends with `esp_restart()`.

- [ ] **Step 1: Add URL-decode helper and form parsing**

Add to `src/esp32/provisioning.rs`:

```rust
/// Decode percent-encoded form values. Handles %XX and '+' (space).
fn url_decode(input: &str) -> String {
    let mut output = Vec::with_capacity(input.len());
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        match b {
            b'+' => output.push(b' '),
            b'%' => {
                let hi = chars.next().and_then(|c| (c as char).to_digit(16));
                let lo = chars.next().and_then(|c| (c as char).to_digit(16));
                if let (Some(h), Some(l)) = (hi, lo) {
                    output.push((h * 16 + l) as u8);
                }
            }
            _ => output.push(b),
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

/// Parse application/x-www-form-urlencoded body into BoardConfig.
fn parse_form_body(body: &str) -> BoardConfig {
    let mut wifi_ssid = String::new();
    let mut wifi_pass = String::new();
    let mut lichess_token = None;
    let mut lichess_level = BoardConfig::DEFAULT_LEVEL;

    for pair in body.split('&') {
        let mut kv = pair.splitn(2, '=');
        let key = kv.next().unwrap_or("");
        let val = kv.next().unwrap_or("");
        let decoded = url_decode(val);
        match key {
            "wifi_ssid" => wifi_ssid = decoded,
            "wifi_pass" => wifi_pass = decoded,
            "lichess_token" if !decoded.is_empty() => lichess_token = Some(decoded),
            "lichess_level" => {
                lichess_level = decoded.parse().unwrap_or(BoardConfig::DEFAULT_LEVEL);
            }
            _ => {}
        }
    }

    BoardConfig {
        wifi_ssid,
        wifi_pass,
        lichess_token,
        lichess_level,
    }
}
```

- [ ] **Step 2: Add SoftAP startup helper**

```rust
use esp_idf_svc::wifi::{AccessPointConfiguration, AuthMethod, Configuration, EspWifi};

/// Start WiFi in SoftAP mode. Returns the EspWifi handle (must be kept alive).
fn start_softap(
    modem: impl esp_idf_svc::hal::modem::WifiModemPeripheral + 'static,
    sys_loop: esp_idf_svc::eventloop::EspSystemEventLoop,
    nvs_partition: esp_idf_svc::nvs::EspDefaultNvsPartition,
) -> Result<EspWifi<'static>, ProvisioningError> {
    let mut wifi = EspWifi::new(modem, sys_loop, Some(nvs_partition))
        .map_err(|e| ProvisioningError::Nvs(e))?;

    let ap_config = AccessPointConfiguration {
        ssid: "ChessBoard".try_into().unwrap(),
        auth_method: AuthMethod::None,
        ..Default::default()
    };

    wifi.set_configuration(&Configuration::AccessPoint(ap_config))
        .map_err(|e| ProvisioningError::Nvs(e))?;
    wifi.start()
        .map_err(|e| ProvisioningError::Nvs(e))?;

    log::info!("SoftAP started: ChessBoard");
    Ok(wifi)
}
```

- [ ] **Step 3: Implement `run_provisioning_server`**

```rust
use esp_idf_svc::http::server::{EspHttpServer, Configuration as HttpConfig};
use esp_idf_svc::hal::delay::FreeRtos;
use std::sync::{Arc, Mutex};

const FORM_HTML: &str = include_str!("provisioning.html");

/// Run the provisioning server. Never returns — reboots after successful config save.
pub fn run_provisioning_server(
    display: &mut impl crate::BoardDisplay,
    modem: impl esp_idf_svc::hal::modem::WifiModemPeripheral + 'static,
    sys_loop: esp_idf_svc::eventloop::EspSystemEventLoop,
    nvs_partition: esp_idf_svc::nvs::EspDefaultNvsPartition,
    nvs: EspNvs<NvsDefault>,
) -> ! {
    use crate::feedback::{BoardFeedback, StatusKind};

    // Show provisioning status on LEDs
    if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Pending)) {
        log::warn!("LED update failed: {e}");
    }

    // Start SoftAP
    let _wifi = start_softap(modem, sys_loop, nvs_partition)
        .expect("failed to start SoftAP");

    // Wrap NVS in Arc<Mutex> so HTTP handlers can access it
    let nvs = Arc::new(Mutex::new(nvs));

    // Flag set by POST handler to trigger reboot
    let reboot_flag = Arc::new(Mutex::new(false));

    let mut server = EspHttpServer::new(&HttpConfig::default())
        .expect("failed to start HTTP server");

    // GET / — serve the form
    server.fn_handler("/", embedded_svc::http::Method::Get, |req| {
        req.into_ok_response()?
            .write_all(FORM_HTML.as_bytes())?;
        Ok(())
    }).expect("failed to register GET handler");

    // POST / — handle form submission
    let nvs_clone = nvs.clone();
    let reboot_clone = reboot_flag.clone();
    server.fn_handler("/", embedded_svc::http::Method::Post, move |mut req| {
        // Read POST body
        let mut body_buf = [0u8; 512];
        let len = req.read(&mut body_buf).unwrap_or(0);
        let body = core::str::from_utf8(&body_buf[..len]).unwrap_or("");

        let config = parse_form_body(body);

        match config.validate() {
            Ok(()) => {
                let nvs = nvs_clone.lock().unwrap();
                if let Err(e) = config.save(&nvs) {
                    let error_html = FORM_HTML.replace(
                        "<!-- ERRORS -->",
                        &format!("<div class=\"error\">Save failed: {e}</div>"),
                    );
                    req.into_ok_response()?
                        .write_all(error_html.as_bytes())?;
                } else {
                    let success = "<!DOCTYPE html><html><body><h1>Saved!</h1><p>Board is rebooting...</p></body></html>";
                    req.into_ok_response()?
                        .write_all(success.as_bytes())?;
                    *reboot_clone.lock().unwrap() = true;
                }
            }
            Err(e) => {
                let error_html = FORM_HTML.replace(
                    "<!-- ERRORS -->",
                    &format!("<div class=\"error\">{e}</div>"),
                );
                req.into_ok_response()?
                    .write_all(error_html.as_bytes())?;
            }
        }
        Ok(())
    }).expect("failed to register POST handler");

    log::info!("Provisioning server running at http://192.168.4.1/");

    // Block until reboot flag is set
    loop {
        if *reboot_flag.lock().unwrap() {
            log::info!("Config saved, rebooting...");
            FreeRtos::delay_ms(1000); // Let the HTTP response flush
            unsafe { esp_idf_svc::sys::esp_restart(); }
        }
        FreeRtos::delay_ms(100);
    }
}
```

- [ ] **Step 4: Verify ESP32 build**

Run: `just build`
Expected: compiles successfully

- [ ] **Step 5: Commit**

Message: `feat: add SoftAP provisioning server`

---
