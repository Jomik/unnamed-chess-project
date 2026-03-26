use std::sync::{Arc, Mutex};

use embedded_svc::io::Read;
use embedded_svc::io::Write;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};
use esp_idf_svc::nvs::{EspNvs, NvsDefault};
use esp_idf_svc::wifi::{AccessPointConfiguration, AuthMethod, Configuration, EspWifi};

use crate::provisioning::{BoardConfig, ValidationError};

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

        if let Some(ref token) = self.lichess_token {
            nvs.set_str(KEY_LICHESS_TOKEN, token)
                .map_err(ProvisioningError::Nvs)?;
        }

        nvs.set_u8(KEY_LICHESS_LEVEL, self.lichess_level)
            .map_err(ProvisioningError::Nvs)?;

        Ok(())
    }
}

const FORM_HTML: &str = include_str!("provisioning.html");

/// Parse application/x-www-form-urlencoded body into BoardConfig.
fn parse_form_body(body: &str) -> BoardConfig {
    let mut wifi_ssid = String::new();
    let mut wifi_pass = String::new();
    let mut lichess_token = None;
    let mut lichess_level = BoardConfig::DEFAULT_LEVEL;

    for (key, val) in form_urlencoded::parse(body.as_bytes()) {
        match key.as_ref() {
            "wifi_ssid" => wifi_ssid = val.into_owned(),
            "wifi_pass" => wifi_pass = val.into_owned(),
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
        lichess_token,
        lichess_level,
    }
}

/// Start WiFi in SoftAP mode. Returns the EspWifi handle (must be kept alive).
fn start_softap(
    modem: impl esp_idf_svc::hal::modem::WifiModemPeripheral + 'static,
    sys_loop: esp_idf_svc::eventloop::EspSystemEventLoop,
    nvs_partition: esp_idf_svc::nvs::EspDefaultNvsPartition,
) -> Result<EspWifi<'static>, ProvisioningError> {
    let mut wifi =
        EspWifi::new(modem, sys_loop, Some(nvs_partition)).map_err(ProvisioningError::Nvs)?;

    let ap_config = AccessPointConfiguration {
        ssid: "ChessBoard".try_into().expect("SSID is valid"),
        auth_method: AuthMethod::None,
        ..Default::default()
    };

    wifi.set_configuration(&Configuration::AccessPoint(ap_config))
        .map_err(ProvisioningError::Nvs)?;
    wifi.start().map_err(ProvisioningError::Nvs)?;

    Ok(wifi)
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

    // Show provisioning status on LEDs
    if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Pending)) {
        log::warn!("LED update failed: {e}");
    }

    // Start SoftAP
    let _wifi = start_softap(modem, sys_loop, nvs_partition).expect("failed to start SoftAP");

    // Wrap NVS in Arc<Mutex> so HTTP handlers can access it
    let nvs = Arc::new(Mutex::new(nvs));

    // Flag set by POST handler to trigger reboot
    let reboot_flag = Arc::new(Mutex::new(false));

    let mut server =
        EspHttpServer::new(&HttpConfig::default()).expect("failed to start HTTP server");

    // GET / — serve the form
    server
        .fn_handler("/", embedded_svc::http::Method::Get, |req| {
            req.into_ok_response()?.write_all(FORM_HTML.as_bytes())?;
            Ok(())
        })
        .expect("failed to register GET handler");

    // POST / — handle form submission
    let nvs_clone = nvs.clone();
    let reboot_clone = reboot_flag.clone();
    server
        .fn_handler(
            "/",
            embedded_svc::http::Method::Post,
            move |mut req| {
                // Read POST body (loop to handle partial reads)
                let mut body_buf = [0u8; 1024];
                let mut total = 0;
                loop {
                    let n = req.read(&mut body_buf[total..]).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    total += n;
                }
                let body = core::str::from_utf8(&body_buf[..total]).unwrap_or("");

                let config = parse_form_body(body);

                match config.validate() {
                    Ok(()) => {
                        let nvs = nvs_clone
                            .lock()
                            .expect("NVS mutex should not be poisoned");
                        if let Err(e) = config.save(&nvs) {
                            let error_html = FORM_HTML.replace(
                                "<!-- ERRORS -->",
                                &format!("<div class=\"error\">Save failed: {e}</div>"),
                            );
                            req.into_ok_response()?.write_all(error_html.as_bytes())?;
                        } else {
                            let success = "<!DOCTYPE html><html><body><h1>Saved!</h1><p>Board is rebooting...</p></body></html>";
                            req.into_ok_response()?.write_all(success.as_bytes())?;
                            *reboot_clone
                                .lock()
                                .expect("reboot mutex should not be poisoned") = true;
                        }
                    }
                    Err(e) => {
                        let error_html = FORM_HTML.replace(
                            "<!-- ERRORS -->",
                            &format!("<div class=\"error\">{e}</div>"),
                        );
                        req.into_ok_response()?.write_all(error_html.as_bytes())?;
                    }
                }
                Ok(())
            },
        )
        .expect("failed to register POST handler");

    log::info!("Provisioning server running at http://192.168.4.1/");

    // Block until reboot flag is set
    loop {
        if *reboot_flag
            .lock()
            .expect("reboot mutex should not be poisoned")
        {
            log::info!("Config saved, rebooting...");
            if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Success)) {
                log::warn!("LED update failed: {e}");
            }
            FreeRtos::delay_ms(1000); // Let the HTTP response flush
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
        assert_eq!(config.lichess_level, BoardConfig::DEFAULT_LEVEL);
        assert!(config.lichess_token.is_none());
    }
}
