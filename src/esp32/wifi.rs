use core::convert::TryInto;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::WifiModemPeripheral;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WifiError {
    #[error("WiFi driver init failed: {0}")]
    DriverInit(String),
    #[error("WiFi configuration failed: {0}")]
    Configuration(String),
    #[error("WiFi start failed: {0}")]
    Start(String),
    #[error("WiFi connect failed: {0}")]
    Connect(String),
    #[error("WiFi waiting for IP failed: {0}")]
    WaitForIp(String),
}

/// Blocking WiFi connection. Dropping disconnects.
pub struct WifiConnection<'d> {
    _wifi: BlockingWifi<EspWifi<'d>>,
}

impl WifiConnection<'_> {
    /// Connect to a WPA2 network and block until an IP is acquired.
    pub fn connect(
        modem: impl WifiModemPeripheral + 'static,
        sys_loop: EspSystemEventLoop,
        nvs: EspDefaultNvsPartition,
        ssid: &str,
        password: &str,
    ) -> Result<Self, WifiError> {
        let esp_wifi = EspWifi::new(modem, sys_loop.clone(), Some(nvs))
            .map_err(|e| WifiError::DriverInit(e.to_string()))?;

        let mut wifi = BlockingWifi::wrap(esp_wifi, sys_loop)
            .map_err(|e| WifiError::DriverInit(e.to_string()))?;

        let config = Configuration::Client(ClientConfiguration {
            ssid: ssid.try_into().map_err(|_| {
                WifiError::Configuration(format!("SSID too long (max 32 bytes): {ssid}"))
            })?,
            auth_method: AuthMethod::WPA2Personal,
            password: password.try_into().map_err(|_| {
                WifiError::Configuration("password too long (max 64 bytes)".to_string())
            })?,
            ..Default::default()
        });

        wifi.set_configuration(&config)
            .map_err(|e| WifiError::Configuration(e.to_string()))?;

        wifi.start().map_err(|e| WifiError::Start(e.to_string()))?;
        log::info!("WiFi started");

        wifi.connect()
            .map_err(|e| WifiError::Connect(e.to_string()))?;
        log::info!("WiFi connected to {ssid}");

        wifi.wait_netif_up()
            .map_err(|e| WifiError::WaitForIp(e.to_string()))?;

        let ip_info = wifi
            .wifi()
            .sta_netif()
            .get_ip_info()
            .map_err(|e| WifiError::WaitForIp(e.to_string()))?;
        log::info!("WiFi got IP: {}", ip_info.ip);

        Ok(Self { _wifi: wifi })
    }
}
