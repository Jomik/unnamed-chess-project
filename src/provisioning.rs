/// Configuration loaded from NVS at boot.
///
/// Validated before saving to NVS. See [`ValidationError`] for constraints.
#[derive(Debug, Clone)]
pub struct BoardConfig {
    pub wifi_ssid: String,
    pub wifi_pass: String,
    pub lichess_token: Option<String>,
    pub lichess_level: u8,
}

/// Validation errors for [`BoardConfig`] fields.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("WiFi SSID must not be empty")]
    SsidEmpty,
    #[error("WiFi SSID exceeds 32 bytes")]
    SsidTooLong,
    #[error("WiFi password must not be empty")]
    PasswordEmpty,
    #[error("WiFi password exceeds 64 bytes")]
    PasswordTooLong,
    #[error("Lichess AI level must be 1–8")]
    LevelOutOfRange,
}

impl BoardConfig {
    pub const DEFAULT_LEVEL: u8 = 2;

    /// Validate all fields. Returns the first error found.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.wifi_ssid.is_empty() {
            return Err(ValidationError::SsidEmpty);
        }
        if self.wifi_ssid.len() > 32 {
            return Err(ValidationError::SsidTooLong);
        }
        if self.wifi_pass.is_empty() {
            return Err(ValidationError::PasswordEmpty);
        }
        if self.wifi_pass.len() > 64 {
            return Err(ValidationError::PasswordTooLong);
        }
        if self.lichess_level < 1 || self.lichess_level > 8 {
            return Err(ValidationError::LevelOutOfRange);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> BoardConfig {
        BoardConfig {
            wifi_ssid: "MyNetwork".into(),
            wifi_pass: "secret123".into(),
            lichess_token: None,
            lichess_level: 2,
        }
    }

    #[test]
    fn valid_config_passes() {
        assert!(valid_config().validate().is_ok());
    }

    #[test]
    fn empty_ssid_rejected() {
        let mut c = valid_config();
        c.wifi_ssid = String::new();
        assert!(matches!(c.validate(), Err(ValidationError::SsidEmpty)));
    }

    #[test]
    fn ssid_over_32_bytes_rejected() {
        let mut c = valid_config();
        c.wifi_ssid = "a".repeat(33);
        assert!(matches!(c.validate(), Err(ValidationError::SsidTooLong)));
    }

    #[test]
    fn ssid_exactly_32_bytes_accepted() {
        let mut c = valid_config();
        c.wifi_ssid = "a".repeat(32);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn empty_password_rejected() {
        let mut c = valid_config();
        c.wifi_pass = String::new();
        assert!(matches!(c.validate(), Err(ValidationError::PasswordEmpty)));
    }

    #[test]
    fn password_over_64_bytes_rejected() {
        let mut c = valid_config();
        c.wifi_pass = "a".repeat(65);
        assert!(matches!(
            c.validate(),
            Err(ValidationError::PasswordTooLong)
        ));
    }

    #[test]
    fn password_exactly_64_bytes_accepted() {
        let mut c = valid_config();
        c.wifi_pass = "a".repeat(64);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn level_zero_rejected() {
        let mut c = valid_config();
        c.lichess_level = 0;
        assert!(matches!(
            c.validate(),
            Err(ValidationError::LevelOutOfRange)
        ));
    }

    #[test]
    fn level_nine_rejected() {
        let mut c = valid_config();
        c.lichess_level = 9;
        assert!(matches!(
            c.validate(),
            Err(ValidationError::LevelOutOfRange)
        ));
    }

    #[test]
    fn level_boundaries_accepted() {
        let mut c = valid_config();
        c.lichess_level = 1;
        assert!(c.validate().is_ok());
        c.lichess_level = 8;
        assert!(c.validate().is_ok());
    }

    #[test]
    fn none_lichess_token_valid() {
        let c = valid_config();
        assert!(c.lichess_token.is_none());
        assert!(c.validate().is_ok());
    }

    #[test]
    fn some_lichess_token_valid() {
        let mut c = valid_config();
        c.lichess_token = Some("lip_abc123".into());
        assert!(c.validate().is_ok());
    }

    #[test]
    fn default_level_is_two() {
        assert_eq!(BoardConfig::DEFAULT_LEVEL, 2);
    }
}
