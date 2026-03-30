# WiFi Trial Connection Design

**Date:** 2026-03-30
**Status:** Implemented

## Motivation

The original SoftAP provisioning form (spec 2026-03-26) required the user to know their WiFi authentication method in advance. In practice, most users do not know whether their network uses WPA2 or WPA3. Additionally, there was no feedback on whether the supplied credentials were correct — the board saved them to NVS and rebooted, only failing silently at the next boot.

This design extends provisioning with a trial connection step: when the user submits the form, the board scans for the target SSID, automatically detects the authentication method, attempts a live connection, and only saves credentials to NVS if the connection succeeds. If the network is not found or the connection fails, the user sees an inline error with options to retry or save anyway.

## WiFi Mode Strategy

The provisioning server starts in **Mixed (AP+STA) mode from the beginning** — not AP-only with a mode switch on form submission. The STA interface sits idle until a scan or connection is requested. This avoids a disruptive mode switch during HTTP request handling that would drop the phone's connection to the AP.

On a trial connection failure, the server stays in Mixed mode. It does not revert to AP-only mode; instead it resets the STA interface to a default `ClientConfiguration` while keeping the AP configuration active, so the `Configuration::Mixed(idle_client, ap_config)` state is preserved.

## `BoardConfig` Changes

`BoardConfig` gains a `wifi_auth: u8` field:

```rust
pub struct BoardConfig {
    pub wifi_ssid: String,
    pub wifi_pass: String,
    pub wifi_auth: u8,   // 0 = Open, 3 = WPA2Personal, 6 = WPA3Personal
    pub lichess_token: Option<String>,
    pub lichess_level: u8,
}
```

The `wifi_auth` field stores the auth method as a numeric code matching the discriminant values used by `esp_idf_svc::wifi::AuthMethod`:

| Value | Meaning              |
|-------|----------------------|
| 0     | Open (no password)   |
| 3     | WPA2Personal         |
| 6     | WPA3Personal         |

Unknown values default to `WPA2Personal` at runtime.

### Validation

`BoardConfig::validate()` allows an empty password only when `wifi_auth == 0` (open network). For `wifi_auth == 3` or `wifi_auth == 6`, an empty password returns `ValidationError::PasswordEmpty`.

```rust
if self.wifi_pass.is_empty() && self.wifi_auth != 0 {
    return Err(ValidationError::PasswordEmpty);
}
```

## NVS Storage

`wifi_auth` is stored as a `u8` under the key `"wifi_auth"` in the `"config"` NVS namespace. During `BoardConfig::load()`, if `wifi_auth` is missing from NVS (i.e., `get_u8` returns `Ok(None)`), the function returns `Ok(None)` — the same as a missing SSID. This triggers re-provisioning, ensuring boards upgraded from the older schema (which lacked `wifi_auth`) re-enter provisioning mode cleanly rather than booting with an incorrect auth method.

## `u8_to_auth_method`

A public conversion function shared between the provisioning server and `main.rs`:

```rust
pub fn u8_to_auth_method(v: u8) -> AuthMethod {
    match v {
        0 => AuthMethod::None,
        3 => AuthMethod::WPA2Personal,
        6 => AuthMethod::WPA3Personal,
        _ => AuthMethod::WPA2Personal,
    }
}
```

`main.rs` calls this at normal boot time to convert the stored `wifi_auth` field before passing it to `WifiConnection::connect`.

## `WifiConnection::connect` Signature

`WifiConnection::connect` takes an explicit `auth_method: AuthMethod` parameter instead of hardcoding `WPA2Personal`:

```rust
pub fn connect(
    modem: impl WifiModemPeripheral + 'static,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    ssid: &str,
    password: &str,
    auth_method: AuthMethod,
) -> Result<Self, WifiError>
```

## Trial Connection Flow

When the user submits `POST /`, the provisioning server:

1. Parses the form body into a `BoardConfig`.
2. Validates the config; returns an inline error on failure.
3. Calls `try_connect(wifi, config, ap_config)`:
   a. Scans for visible access points (`wifi.scan()`).
   b. Looks up the target SSID in scan results.
   c. If not found → `TrialResult::NotFound`.
   d. If found but `ap.auth_method` is `None` → `TrialResult::NotFound` (treated as unknown/hidden).
   e. If found with a known `auth_method` → updates `config.wifi_auth` from the scan result, then calls `attempt_sta_connection`.
4. On `TrialResult::Success`: saves to NVS, sets reboot flag, and returns without sending a response body (see note below).
5. On `TrialResult::ConnectFailed`: renders trial error page with "Save Anyway" option.
6. On `TrialResult::NotFound`: renders not-found page with auth dropdown and both Retry and Save Anyway options.

### Success Response Omission

When `TrialResult::Success` is matched in the `POST /` or `POST /retry` handler, the handler saves the config to NVS, sets the reboot flag, calls `req.into_ok_response()` to satisfy the HTTP framework, and returns immediately — **without writing a response body**.

The reason: the trial connection involves a channel switch (the ESP32's radio joins the user's AP on its own channel), which breaks the phone's TCP connection to the SoftAP. By the time the handler returns, the phone is already disconnected. Attempting to render and send a success page would fail silently. Sending an empty 200 response keeps the HTTP framework happy while avoiding the dead write. The user will see their phone lose the ChessBoard network and reconnect to their home network — which is the expected indication that provisioning succeeded.

`POST /save` (which does not perform a trial connection) still renders and sends the success page because the phone remains connected on the same channel throughout that flow.

### `AccessPointInfo.auth_method`

The `auth_method` field on scan results is `Option<AuthMethod>`. The trial logic treats `None` the same as not finding the SSID at all (returns `NotFound`), because a missing auth method means the board cannot determine how to connect.

### Connection Reset on Failure

When `wifi.connect()` or `wifi.wait_netif_up()` fails:

```rust
let _ = wifi.disconnect();
let idle = ClientConfiguration::default();
let _ = wifi.set_configuration(&Configuration::Mixed(idle, ap_config.clone()));
```

This resets the STA to idle while keeping the AP alive, so the user can correct their credentials and try again.

## HTTP Endpoints

| Method | Path     | Purpose                                                   |
|--------|----------|-----------------------------------------------------------|
| GET    | `/`      | Serve blank config form                                   |
| POST   | `/`      | Scan + trial connect; save on success                     |
| POST   | `/retry` | Explicit auth trial for hidden networks                   |
| POST   | `/save`  | Save without verification (bypass trial)                  |

### POST / (scan + trial)

Reads the form body, validates, scans for the SSID, detects auth method from scan results, attempts connection. On success: save + reboot. On failure: render error with Save Anyway form. On not-found: render not-found page with Retry form.

### POST /retry (hidden network retry)

For networks not visible in scans. The user selects an auth method from a dropdown and submits. Goes straight to `attempt_sta_connection` using `u8_to_auth_method(config.wifi_auth)` — no scan step.

### POST /save (save without verification)

Saves the config directly to NVS without a trial connection. Used as an escape hatch when the trial fails and the user wants to persist their credentials anyway. Does not validate that the connection works.

## HTML and Rendering

The form HTML is embedded via `include_str!("provisioning.html")`. Template substitution uses HTML comment placeholders:

| Placeholder              | Replaced with                        |
|--------------------------|--------------------------------------|
| `<!-- STATUS -->`        | Status/error message block           |
| `<!-- ERRORS -->`        | Inline validation error block        |
| `<!-- VAL_wifi_ssid -->` | HTML-escaped SSID value              |
| `<!-- VAL_wifi_pass -->` | HTML-escaped password value          |
| `<!-- VAL_lichess_token -->` | HTML-escaped token value         |
| `<!-- VAL_lichess_level -->` | Lichess level as string          |

All render functions HTML-escape user-supplied values via `html_escape()`, which replaces `&`, `<`, `>`, and `"`.

### render_not_found

Renders two forms: a `/retry` form with an auth method dropdown (WPA2 default, WPA3, Open), and a `/save` form. A small inline `<script>` block synchronizes the auth dropdown value into a hidden field on the Save Anyway form, so both forms use the same selected auth method.

### render_trial_error

Renders an error message and a `/save` form with all config fields as hidden inputs.

### render_success

Hides the main form (`style="display:none"`) and shows a success message confirming the SSID and that the board is rebooting.

## Form Behavior

The main form's submit button is disabled via JavaScript after click and its label changes to `"Scanning for <ssid>..."` while the trial connection proceeds. This prevents double-submission and gives the user feedback that work is in progress.

## Reboot Mechanism

The provisioning server loop checks a shared `reboot_flag: Arc<Mutex<bool>>` every 100ms. When set to `true` by a successful save, the loop shows the success LED pattern, delays 1 second, and calls `esp_restart()`.

For `POST /save` (save without trial), the flag is set inside the HTTP handler before the response is written, and the 1-second delay in the reboot loop ensures the success page reaches the phone before the board restarts.

For `POST /` and `POST /retry` (trial connection paths), the phone's connection is already broken by the WiFi channel switch before the handler sets the reboot flag. No response body is sent to the phone; the 1-second delay still allows any pending network teardown to complete cleanly before the restart.

## Testing Strategy

### Host-Testable (unit tests in `src/esp32/provisioning.rs`)

- `parse_form_body`: field extraction, URL decoding, default values, unknown keys.
- `html_escape`: special character substitution.
- `render_error`: error div injection, placeholder replacement.
- `render_with_values`: config value injection with escaping.
- `render_success`: form hidden, SSID escaped, placeholders cleared.
- `render_trial_error`: error message, Save Anyway form presence.
- `render_not_found`: not-found message, Retry form, Save Anyway form, auth dropdown.
- `u8_to_auth_method`: all mapped values and unknown fallback.

### Hardware-Only (manual testing)

- SoftAP startup and HTTP form serving at `192.168.71.1`.
- Successful trial connection: NVS save, reboot, normal boot with stored credentials.
- Trial connection failure: error page with Save Anyway.
- SSID not found: not-found page with Retry and Save Anyway.
- Hidden network via `/retry` endpoint.
- Save Anyway from not-found page: NVS save, reboot.
- Open network (auth=0) with empty password accepted.
- Boards upgraded from pre-`wifi_auth` NVS schema re-enter provisioning mode.
