use esp_idf_svc::nvs::{EspNvs, NvsDefault};

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
