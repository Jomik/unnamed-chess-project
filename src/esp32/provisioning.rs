use std::sync::{Arc, Mutex};

use embedded_svc::io::Write;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};
use esp_idf_svc::nvs::{EspNvs, EspNvsPartition, NvsCustom, NvsDefault};
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi,
};

use crate::esp32::config::{SensorCalibration, SensorConfig};
use crate::provisioning::{BoardConfig, ValidationError};

const KEY_WIFI_SSID: &str = "wifi_ssid";
const KEY_WIFI_PASS: &str = "wifi_pass";
const KEY_WIFI_AUTH: &str = "wifi_auth";
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

impl BoardConfig {
    /// Load config from NVS. Returns `Ok(None)` if wifi_ssid is missing
    /// (first boot / after erase). Returns `Err` on NVS read failures.
    pub fn load(nvs: &EspNvs<NvsDefault>) -> Result<Option<Self>, ProvisioningError> {
        let mut ssid_buf = [0u8; BUF_SSID];
        let wifi_ssid = match nvs
            .get_str(KEY_WIFI_SSID, &mut ssid_buf)
            .map_err(ProvisioningError::Nvs)?
        {
            Some(s) => s.to_string(),
            None => return Ok(None),
        };

        let mut pass_buf = [0u8; BUF_PASS];
        let wifi_pass = nvs
            .get_str(KEY_WIFI_PASS, &mut pass_buf)
            .map_err(ProvisioningError::Nvs)?
            .unwrap_or("")
            .to_string();

        let wifi_auth = match nvs.get_u8(KEY_WIFI_AUTH).map_err(ProvisioningError::Nvs)? {
            Some(v) => v,
            None => return Ok(None),
        };

        let mut token_buf = [0u8; BUF_TOKEN];
        let lichess_token = nvs
            .get_str(KEY_LICHESS_TOKEN, &mut token_buf)
            .map_err(ProvisioningError::Nvs)?
            .map(|s| s.to_string());

        let lichess_level = nvs
            .get_u8(KEY_LICHESS_LEVEL)
            .map_err(ProvisioningError::Nvs)?
            .unwrap_or(BoardConfig::DEFAULT_LEVEL);

        Ok(Some(BoardConfig {
            wifi_ssid,
            wifi_pass,
            wifi_auth,
            lichess_token,
            lichess_level,
        }))
    }

    /// Validate and save config to NVS.
    pub fn save(&self, nvs: &EspNvs<NvsDefault>) -> Result<(), ProvisioningError> {
        self.validate()?;

        nvs.set_str(KEY_WIFI_SSID, &self.wifi_ssid)
            .map_err(ProvisioningError::Nvs)?;
        nvs.set_str(KEY_WIFI_PASS, &self.wifi_pass)
            .map_err(ProvisioningError::Nvs)?;
        nvs.set_u8(KEY_WIFI_AUTH, self.wifi_auth)
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

const CAL_NAMESPACE: &str = "cal";
const KEY_CAL_BASELINE: &str = "cal_baseline";
const KEY_CAL_THRESHOLD: &str = "cal_threshold";

impl SensorCalibration {
    pub fn load(partition: &EspNvsPartition<NvsCustom>) -> Result<Option<Self>, ProvisioningError> {
        let nvs = match EspNvs::new(partition.clone(), CAL_NAMESPACE, false) {
            Ok(nvs) => nvs,
            Err(e) if e.code() == esp_idf_svc::sys::ESP_ERR_NVS_NOT_FOUND => return Ok(None),
            Err(e) => return Err(ProvisioningError::Nvs(e)),
        };

        let baseline_mv = match nvs
            .get_u16(KEY_CAL_BASELINE)
            .map_err(ProvisioningError::Nvs)?
        {
            Some(v) => v,
            None => return Ok(None),
        };

        let threshold_mv = nvs
            .get_u16(KEY_CAL_THRESHOLD)
            .map_err(ProvisioningError::Nvs)?
            .unwrap_or(SensorConfig::default().threshold_mv);

        Ok(Some(SensorCalibration {
            baseline_mv,
            threshold_mv,
        }))
    }

    pub fn save(&self, partition: &EspNvsPartition<NvsCustom>) -> Result<(), ProvisioningError> {
        let nvs =
            EspNvs::new(partition.clone(), CAL_NAMESPACE, true).map_err(ProvisioningError::Nvs)?;
        nvs.set_u16(KEY_CAL_BASELINE, self.baseline_mv)
            .map_err(ProvisioningError::Nvs)?;
        nvs.set_u16(KEY_CAL_THRESHOLD, self.threshold_mv)
            .map_err(ProvisioningError::Nvs)?;
        Ok(())
    }
}

const FORM_HTML: &str = include_str!("provisioning.html");

/// Parse application/x-www-form-urlencoded body into BoardConfig.
fn parse_form_body(body: &str) -> BoardConfig {
    let mut wifi_ssid = String::new();
    let mut wifi_pass = String::new();
    let mut wifi_auth: u8 = 3; // default WPA2
    let mut lichess_token = None;
    let mut lichess_level = BoardConfig::DEFAULT_LEVEL;

    for (key, val) in form_urlencoded::parse(body.as_bytes()) {
        match key.as_ref() {
            "wifi_ssid" => wifi_ssid = val.into_owned(),
            "wifi_pass" => wifi_pass = val.into_owned(),
            "wifi_auth" => wifi_auth = val.parse().unwrap_or(3),
            "lichess_token" if !val.is_empty() => lichess_token = Some(val.into_owned()),
            "lichess_level" => {
                lichess_level = val.parse().unwrap_or(BoardConfig::DEFAULT_LEVEL);
            }
            _ => {}
        }
    }

    BoardConfig {
        wifi_ssid,
        wifi_pass,
        wifi_auth,
        lichess_token,
        lichess_level,
    }
}

fn render_error(msg: &str) -> String {
    FORM_HTML
        .replace(
            "<!-- ERRORS -->",
            &format!("<div class=\"error\">{}</div>", html_escape(msg)),
        )
        .replace("<!-- STATUS -->", "")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_with_values(html: &str, config: &BoardConfig) -> String {
    html.replace("<!-- VAL_wifi_ssid -->", &html_escape(&config.wifi_ssid))
        .replace("<!-- VAL_wifi_pass -->", &html_escape(&config.wifi_pass))
        .replace(
            "<!-- VAL_lichess_token -->",
            &html_escape(config.lichess_token.as_deref().unwrap_or("")),
        )
        .replace(
            "<!-- VAL_lichess_level -->",
            &config.lichess_level.to_string(),
        )
}

const SUCCESS_TEMPLATE: &str = r#"<div class="success">Connected to "<!-- SSID -->"! Configuration saved. Board is rebooting...</div>"#;

fn render_success(ssid: &str) -> String {
    let status = SUCCESS_TEMPLATE.replace("<!-- SSID -->", &html_escape(ssid));
    let html = FORM_HTML
        .replace("<!-- STATUS -->", &status)
        .replace("<!-- ERRORS -->", "");
    html.replace("<form", "<form style=\"display:none\"")
}

fn render_trial_error(config: &BoardConfig, error: &str) -> String {
    let status = format!(
        r#"<div class="error">Could not connect to "{}": {}</div>
<form method="POST" action="/save">
<input type="hidden" name="wifi_ssid" value="{}">
<input type="hidden" name="wifi_pass" value="{}">
<input type="hidden" name="wifi_auth" value="{}">
<input type="hidden" name="lichess_token" value="{}">
<input type="hidden" name="lichess_level" value="{}">
<button type="submit">Save Anyway</button>
</form>"#,
        html_escape(&config.wifi_ssid),
        html_escape(error),
        html_escape(&config.wifi_ssid),
        html_escape(&config.wifi_pass),
        config.wifi_auth,
        html_escape(config.lichess_token.as_deref().unwrap_or("")),
        config.lichess_level,
    );
    let html = FORM_HTML
        .replace("<!-- STATUS -->", &status)
        .replace("<!-- ERRORS -->", "");
    render_with_values(&html, config)
}

fn render_not_found(config: &BoardConfig) -> String {
    let status = format!(
        r#"<div class="error">Network "{}" was not found in scan. It may be hidden or out of range.</div>
<form method="POST" action="/retry">
<input type="hidden" name="wifi_ssid" value="{}">
<input type="hidden" name="wifi_pass" value="{}">
<input type="hidden" name="lichess_token" value="{}">
<input type="hidden" name="lichess_level" value="{}">
<label for="wifi_auth">Authentication</label>
<select name="wifi_auth" id="wifi_auth">
<option value="3" selected>WPA2</option>
<option value="6">WPA3</option>
<option value="0">Open (no password)</option>
</select>
<button type="submit">Retry</button>
</form>
<form method="POST" action="/save">
<input type="hidden" name="wifi_ssid" value="{}">
<input type="hidden" name="wifi_pass" value="{}">
<input type="hidden" name="wifi_auth" value="3" id="save-wifi-auth">
<input type="hidden" name="lichess_token" value="{}">
<input type="hidden" name="lichess_level" value="{}">
<button type="submit" id="save-anyway-btn">Save Anyway</button>
</form>
<script>
document.getElementById('wifi_auth').addEventListener('change', function() {{
    document.getElementById('save-wifi-auth').value = this.value;
}});
</script>"#,
        html_escape(&config.wifi_ssid),
        html_escape(&config.wifi_ssid),
        html_escape(&config.wifi_pass),
        html_escape(config.lichess_token.as_deref().unwrap_or("")),
        config.lichess_level,
        html_escape(&config.wifi_ssid),
        html_escape(&config.wifi_pass),
        html_escape(config.lichess_token.as_deref().unwrap_or("")),
        config.lichess_level,
    );
    let html = FORM_HTML
        .replace("<!-- STATUS -->", &status)
        .replace("<!-- ERRORS -->", "");
    render_with_values(&html, config)
}

pub fn u8_to_auth_method(v: u8) -> AuthMethod {
    match v {
        0 => AuthMethod::None,
        3 => AuthMethod::WPA2Personal,
        6 => AuthMethod::WPA3Personal,
        _ => AuthMethod::WPA2Personal,
    }
}

enum TrialResult {
    Success(BoardConfig),
    ConnectFailed(BoardConfig, String),
    NotFound(BoardConfig),
}

fn try_connect(
    wifi: &mut BlockingWifi<EspWifi<'static>>,
    config: &BoardConfig,
    ap_config: &AccessPointConfiguration,
) -> TrialResult {
    let mut config = config.clone();

    // Already in Mixed mode (AP+STA) — scan directly
    // Scan for target SSID
    let scan_results = match wifi.scan() {
        Ok(results) => results,
        Err(e) => return TrialResult::ConnectFailed(config, format!("WiFi scan failed: {e}")),
    };

    let target = scan_results
        .iter()
        .find(|ap| ap.ssid.as_str() == config.wifi_ssid);

    let auth_method = match target {
        Some(ap) => match ap.auth_method {
            Some(method) => method,
            None => return TrialResult::NotFound(config),
        },
        None => return TrialResult::NotFound(config),
    };

    // Update config with discovered auth mode
    config.wifi_auth = match auth_method {
        AuthMethod::None => 0,
        AuthMethod::WPA2Personal => 3,
        AuthMethod::WPA3Personal => 6,
        _ => 3,
    };

    attempt_sta_connection(wifi, &config, ap_config, auth_method)
}

fn attempt_sta_connection(
    wifi: &mut BlockingWifi<EspWifi<'static>>,
    config: &BoardConfig,
    ap_config: &AccessPointConfiguration,
    auth_method: AuthMethod,
) -> TrialResult {
    let client_conf = ClientConfiguration {
        ssid: match config.wifi_ssid.as_str().try_into() {
            Ok(s) => s,
            Err(_) => return TrialResult::ConnectFailed(config.clone(), "SSID too long".into()),
        },
        password: match config.wifi_pass.as_str().try_into() {
            Ok(p) => p,
            Err(_) => {
                return TrialResult::ConnectFailed(config.clone(), "Password too long".into());
            }
        },
        auth_method,
        ..Default::default()
    };

    if let Err(e) = wifi.set_configuration(&Configuration::Mixed(client_conf, ap_config.clone())) {
        return TrialResult::ConnectFailed(config.clone(), format!("WiFi config failed: {e}"));
    }

    if let Err(e) = wifi.connect() {
        let _ = wifi.disconnect();
        // Reset STA to default while keeping AP alive in Mixed mode
        let idle = ClientConfiguration::default();
        let _ = wifi.set_configuration(&Configuration::Mixed(idle, ap_config.clone()));
        return TrialResult::ConnectFailed(config.clone(), format!("Connection failed: {e}"));
    }

    if let Err(e) = wifi.wait_netif_up() {
        let _ = wifi.disconnect();
        let idle = ClientConfiguration::default();
        let _ = wifi.set_configuration(&Configuration::Mixed(idle, ap_config.clone()));
        return TrialResult::ConnectFailed(config.clone(), format!("Failed to acquire IP: {e}"));
    }

    // Credentials verified — disconnect STA so the radio returns to the SoftAP
    // channel. Without this, the channel switch breaks the phone's TCP connection
    // and the HTTP response never arrives.
    let _ = wifi.disconnect();
    let idle = ClientConfiguration::default();
    let _ = wifi.set_configuration(&Configuration::Mixed(idle, ap_config.clone()));

    TrialResult::Success(config.clone())
}

fn read_body(req: &mut impl embedded_svc::io::Read) -> String {
    let mut buf = [0u8; 1024];
    let mut total = 0;
    loop {
        let n = req.read(&mut buf[total..]).unwrap_or(0);
        if n == 0 {
            break;
        }
        total += n;
    }
    core::str::from_utf8(&buf[..total])
        .unwrap_or("")
        .to_string()
}

/// Run the provisioning server. Never returns — reboots after successful config save.
pub fn run_provisioning_server(
    display: &mut impl crate::BoardDisplay,
    modem: impl esp_idf_svc::hal::modem::WifiModemPeripheral + 'static,
    sys_loop: esp_idf_svc::eventloop::EspSystemEventLoop,
    nvs_partition: esp_idf_svc::nvs::EspDefaultNvsPartition,
    nvs: EspNvs<NvsDefault>,
) -> ! {
    use crate::feedback::{BoardFeedback, StatusKind};

    if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Pending)) {
        log::warn!("LED update failed: {e}");
    }

    let ap_config = AccessPointConfiguration {
        ssid: "ChessBoard".try_into().expect("SSID is valid"),
        auth_method: AuthMethod::None,
        ..Default::default()
    };

    let esp_wifi = EspWifi::new(modem, sys_loop.clone(), Some(nvs_partition))
        .expect("WiFi driver init failed");
    let mut wifi = BlockingWifi::wrap(esp_wifi, sys_loop).expect("BlockingWifi wrap failed");

    // Start in Mixed (AP+STA) mode from the beginning. The STA interface sits
    // idle until a scan/connect is requested. This avoids a disruptive mode
    // switch during HTTP requests that would drop the phone's connection.
    let client_conf = ClientConfiguration::default();
    wifi.set_configuration(&Configuration::Mixed(client_conf, ap_config.clone()))
        .expect("WiFi AP+STA config failed");
    wifi.start().expect("WiFi start failed");

    let wifi = Arc::new(Mutex::new(wifi));
    let nvs = Arc::new(Mutex::new(nvs));
    let reboot_flag = Arc::new(Mutex::new(false));
    let ap_config = Arc::new(ap_config);

    let mut server =
        EspHttpServer::new(&HttpConfig::default()).expect("failed to start HTTP server");

    // GET /
    server
        .fn_handler("/", embedded_svc::http::Method::Get, |req| {
            let default_config = BoardConfig {
                wifi_ssid: String::new(),
                wifi_pass: String::new(),
                wifi_auth: 3,
                lichess_token: None,
                lichess_level: BoardConfig::DEFAULT_LEVEL,
            };
            let html = FORM_HTML
                .replace("<!-- STATUS -->", "")
                .replace("<!-- ERRORS -->", "");
            let html = render_with_values(&html, &default_config);
            req.into_ok_response()?.write_all(html.as_bytes())?;
            Ok::<(), Box<dyn std::error::Error>>(())
        })
        .expect("failed to register GET handler");

    // POST / — scan + trial
    {
        let wifi = wifi.clone();
        let nvs = nvs.clone();
        let reboot_flag = reboot_flag.clone();
        let ap_config = ap_config.clone();
        server
            .fn_handler("/", embedded_svc::http::Method::Post, move |mut req| {
                let body = read_body(&mut req);
                let config = parse_form_body(&body);

                if let Err(e) = config.validate() {
                    let html = render_with_values(&render_error(&e.to_string()), &config);
                    req.into_ok_response()?.write_all(html.as_bytes())?;
                    return Ok(());
                }

                let mut wifi = wifi.lock().expect("WiFi mutex poisoned");
                let result = try_connect(&mut wifi, &config, &ap_config);

                if let TrialResult::Success(config) = result {
                    let nvs = nvs.lock().expect("NVS mutex poisoned");
                    if let Err(e) = config.save(&nvs) {
                        log::error!("NVS save failed after successful trial: {e}");
                    } else {
                        log::info!("Trial connection succeeded, saving and rebooting");
                        *reboot_flag.lock().expect("reboot mutex poisoned") = true;
                    }
                    // Phone's connection is broken from the channel switch during
                    // trial — don't try to send a response, just return.
                    req.into_ok_response()?;
                    return Ok(());
                }

                let html = match result {
                    TrialResult::Success(_) => {
                        req.into_ok_response()?;
                        return Ok(());
                    }
                    TrialResult::ConnectFailed(config, error) => {
                        render_trial_error(&config, &error)
                    }
                    TrialResult::NotFound(config) => render_not_found(&config),
                };

                req.into_ok_response()?.write_all(html.as_bytes())?;
                Ok::<(), Box<dyn std::error::Error>>(())
            })
            .expect("failed to register POST handler");
    }

    // POST /retry — explicit auth mode trial (hidden networks)
    {
        let wifi = wifi.clone();
        let nvs = nvs.clone();
        let reboot_flag = reboot_flag.clone();
        let ap_config = ap_config.clone();
        server
            .fn_handler(
                "/retry",
                embedded_svc::http::Method::Post,
                move |mut req| {
                    let body = read_body(&mut req);
                    let config = parse_form_body(&body);

                    if let Err(e) = config.validate() {
                        let html = render_with_values(&render_error(&e.to_string()), &config);
                        req.into_ok_response()?.write_all(html.as_bytes())?;
                        return Ok(());
                    }

                    let auth_method = u8_to_auth_method(config.wifi_auth);

                    let mut wifi = wifi.lock().expect("WiFi mutex poisoned");

                    // Already in Mixed mode — go straight to connection attempt
                    let result =
                        attempt_sta_connection(&mut wifi, &config, &ap_config, auth_method);

                    if let TrialResult::Success(config) = result {
                        let nvs = nvs.lock().expect("NVS mutex poisoned");
                        if let Err(e) = config.save(&nvs) {
                            log::error!("NVS save failed after successful trial: {e}");
                        } else {
                            log::info!("Trial connection succeeded, saving and rebooting");
                            *reboot_flag.lock().expect("reboot mutex poisoned") = true;
                        }
                        // Phone's connection is broken from the channel switch during
                        // trial — don't try to send a response, just return.
                        req.into_ok_response()?;
                        return Ok(());
                    }

                    let html = match result {
                        TrialResult::Success(_) => {
                            req.into_ok_response()?;
                            return Ok(());
                        }
                        TrialResult::ConnectFailed(config, error) => {
                            render_trial_error(&config, &error)
                        }
                        TrialResult::NotFound(config) => {
                            render_trial_error(&config, "Unexpected: network not found")
                        }
                    };

                    req.into_ok_response()?.write_all(html.as_bytes())?;
                    Ok::<(), Box<dyn std::error::Error>>(())
                },
            )
            .expect("failed to register POST /retry handler");
    }

    // POST /save — save without verification
    {
        let nvs = nvs.clone();
        let reboot_flag = reboot_flag.clone();
        server
            .fn_handler("/save", embedded_svc::http::Method::Post, move |mut req| {
                let body = read_body(&mut req);
                let config = parse_form_body(&body);

                let nvs = nvs.lock().expect("NVS mutex poisoned");
                match config.save(&nvs) {
                    Ok(()) => {
                        *reboot_flag.lock().expect("reboot mutex poisoned") = true;
                        req.into_ok_response()?
                            .write_all(render_success(&config.wifi_ssid).as_bytes())?;
                    }
                    Err(e) => {
                        let html = render_error(&format!("Save failed: {e}"));
                        req.into_ok_response()?.write_all(html.as_bytes())?;
                    }
                }
                Ok::<(), Box<dyn std::error::Error>>(())
            })
            .expect("failed to register POST /save handler");
    }

    log::info!("Provisioning server running at http://192.168.71.1/");

    loop {
        if *reboot_flag
            .lock()
            .expect("reboot mutex should not be poisoned")
        {
            log::info!("Config saved, rebooting...");
            if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Success)) {
                log::warn!("LED update failed: {e}");
            }
            FreeRtos::delay_ms(1000);
            unsafe {
                esp_idf_svc::sys::esp_restart();
            }
        }
        FreeRtos::delay_ms(100);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_form_body ---

    fn valid_body() -> &'static str {
        "wifi_ssid=MyNetwork&wifi_pass=secret123&lichess_level=3"
    }

    #[test]
    fn parse_form_body_basic_fields() {
        let config = parse_form_body(valid_body());
        assert_eq!(config.wifi_ssid, "MyNetwork");
        assert_eq!(config.wifi_pass, "secret123");
        assert_eq!(config.lichess_level, 3);
        assert_eq!(config.wifi_auth, 3); // default WPA2
        assert!(config.lichess_token.is_none());
    }

    #[test]
    fn parse_form_body_with_lichess_token() {
        let body = "wifi_ssid=Net&wifi_pass=pass&lichess_token=lip_abc123&lichess_level=5";
        let config = parse_form_body(body);
        assert_eq!(config.lichess_token, Some("lip_abc123".to_string()));
        assert_eq!(config.lichess_level, 5);
    }

    #[test]
    fn parse_form_body_empty_lichess_token_stays_none() {
        let body = "wifi_ssid=Net&wifi_pass=pass&lichess_token=&lichess_level=2";
        let config = parse_form_body(body);
        assert!(config.lichess_token.is_none());
    }

    #[test]
    fn parse_form_body_url_encoded_ssid() {
        let body = "wifi_ssid=My+Network&wifi_pass=secret%21&lichess_level=2";
        let config = parse_form_body(body);
        assert_eq!(config.wifi_ssid, "My Network");
        assert_eq!(config.wifi_pass, "secret!");
    }

    #[test]
    fn parse_form_body_invalid_level_uses_default() {
        let body = "wifi_ssid=Net&wifi_pass=pass&lichess_level=notanumber";
        let config = parse_form_body(body);
        assert_eq!(config.lichess_level, BoardConfig::DEFAULT_LEVEL);
    }

    #[test]
    fn parse_form_body_missing_level_uses_default() {
        let body = "wifi_ssid=Net&wifi_pass=pass";
        let config = parse_form_body(body);
        assert_eq!(config.lichess_level, BoardConfig::DEFAULT_LEVEL);
    }

    #[test]
    fn parse_form_body_unknown_keys_ignored() {
        let body = "wifi_ssid=Net&wifi_pass=pass&unknown_key=value&lichess_level=4";
        let config = parse_form_body(body);
        assert_eq!(config.wifi_ssid, "Net");
        assert_eq!(config.lichess_level, 4);
    }

    #[test]
    fn parse_form_body_empty_body_yields_empty_strings() {
        let config = parse_form_body("");
        assert_eq!(config.wifi_ssid, "");
        assert_eq!(config.wifi_pass, "");
        assert_eq!(config.wifi_auth, 3); // default WPA2
        assert_eq!(config.lichess_level, BoardConfig::DEFAULT_LEVEL);
        assert!(config.lichess_token.is_none());
    }

    #[test]
    fn parse_form_body_with_wifi_auth() {
        let body = "wifi_ssid=Net&wifi_pass=pass&wifi_auth=6&lichess_level=2";
        let config = parse_form_body(body);
        assert_eq!(config.wifi_auth, 6); // WPA3
    }

    #[test]
    fn parse_form_body_invalid_wifi_auth_defaults_to_wpa2() {
        let body = "wifi_ssid=Net&wifi_pass=pass&wifi_auth=invalid";
        let config = parse_form_body(body);
        assert_eq!(config.wifi_auth, 3); // default WPA2
    }

    // --- html_escape ---

    #[test]
    fn html_escape_special_chars() {
        assert_eq!(html_escape("a&b<c>d\"e"), "a&amp;b&lt;c&gt;d&quot;e");
    }

    #[test]
    fn html_escape_no_special_chars() {
        assert_eq!(html_escape("hello world"), "hello world");
    }

    // --- render_error ---

    #[test]
    fn render_error_injects_error_div() {
        let html = render_error("bad input");
        assert!(html.contains("<div class=\"error\">bad input</div>"));
        assert!(!html.contains("<!-- ERRORS -->"));
        assert!(!html.contains("<!-- STATUS -->"));
    }

    // --- render_with_values ---

    #[test]
    fn render_with_values_injects_config() {
        let config = BoardConfig {
            wifi_ssid: "My&Net".to_string(),
            wifi_pass: "sec<ret".to_string(),
            wifi_auth: 3,
            lichess_token: Some("tok\"en".to_string()),
            lichess_level: 5,
        };
        let html = render_with_values(FORM_HTML, &config);
        assert!(html.contains("My&amp;Net"));
        assert!(html.contains("sec&lt;ret"));
        assert!(html.contains("tok&quot;en"));
        assert!(html.contains("5"));
    }

    // --- render_success ---

    #[test]
    fn render_success_shows_ssid_and_hides_form() {
        let html = render_success("TestNet");
        assert!(html.contains("Connected to &quot;TestNet&quot;!"));
        assert!(html.contains("style=\"display:none\""));
        assert!(!html.contains("<!-- STATUS -->"));
        assert!(!html.contains("<!-- ERRORS -->"));
    }

    #[test]
    fn render_success_escapes_ssid() {
        let html = render_success("Net<script>");
        assert!(html.contains("Net&lt;script&gt;"));
        // The SSID should be escaped — no unescaped "<script>" in the status div
        assert!(!html.contains("Net<script>"));
    }

    // --- render_trial_error ---

    #[test]
    fn render_trial_error_shows_save_anyway() {
        let config = BoardConfig {
            wifi_ssid: "Net".to_string(),
            wifi_pass: "pass".to_string(),
            wifi_auth: 3,
            lichess_token: None,
            lichess_level: 2,
        };
        let html = render_trial_error(&config, "timeout");
        assert!(html.contains("Could not connect to &quot;Net&quot;: timeout"));
        assert!(html.contains("Save Anyway"));
        assert!(html.contains("action=\"/save\""));
    }

    // --- render_not_found ---

    #[test]
    fn render_not_found_shows_retry_and_save() {
        let config = BoardConfig {
            wifi_ssid: "HiddenNet".to_string(),
            wifi_pass: "pass".to_string(),
            wifi_auth: 3,
            lichess_token: None,
            lichess_level: 2,
        };
        let html = render_not_found(&config);
        assert!(html.contains("was not found in scan"));
        assert!(html.contains("action=\"/retry\""));
        assert!(html.contains("action=\"/save\""));
        assert!(html.contains("WPA2"));
        assert!(html.contains("WPA3"));
    }

    // --- u8_to_auth_method ---

    #[test]
    fn u8_to_auth_method_maps_correctly() {
        assert_eq!(u8_to_auth_method(0), AuthMethod::None);
        assert_eq!(u8_to_auth_method(3), AuthMethod::WPA2Personal);
        assert_eq!(u8_to_auth_method(6), AuthMethod::WPA3Personal);
        assert_eq!(u8_to_auth_method(99), AuthMethod::WPA2Personal); // unknown defaults to WPA2
    }
}
