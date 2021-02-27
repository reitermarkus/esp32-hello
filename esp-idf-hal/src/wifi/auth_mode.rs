use esp_idf_bindgen::wifi_auth_mode_t;

/// A WiFi authentication mode.
#[derive(Debug, Clone, Copy)]
pub enum AuthMode {
  Open,
  Wep,
  WpaPsk,
  WpaWpa2Psk,
  Wpa2Psk,
  #[cfg(target_device = "esp32")]
  Wpa2Wpa3Psk,
  #[cfg(target_device = "esp32")]
  Wpa3Psk,
  Wpa2Enterprise,
  WapiPsk,
}

impl From<wifi_auth_mode_t> for AuthMode {
  fn from(auth_mode: wifi_auth_mode_t) -> Self {
    match auth_mode {
      wifi_auth_mode_t::WIFI_AUTH_OPEN => AuthMode::Open,
      wifi_auth_mode_t::WIFI_AUTH_WEP => AuthMode::Wep,
      wifi_auth_mode_t::WIFI_AUTH_WPA_PSK => AuthMode::WpaPsk,
      wifi_auth_mode_t::WIFI_AUTH_WPA_WPA2_PSK => AuthMode::WpaWpa2Psk,
      wifi_auth_mode_t::WIFI_AUTH_WPA2_PSK => AuthMode::Wpa2Psk,
      #[cfg(target_device = "esp32")]
      wifi_auth_mode_t::WIFI_AUTH_WPA2_WPA3_PSK => AuthMode::Wpa2Wpa3Psk,
      #[cfg(target_device = "esp32")]
      wifi_auth_mode_t::WIFI_AUTH_WPA3_PSK => AuthMode::Wpa3Psk,
      wifi_auth_mode_t::WIFI_AUTH_WPA2_ENTERPRISE => AuthMode::Wpa2Enterprise,
      wifi_auth_mode_t::WIFI_AUTH_WAPI_PSK => AuthMode::WapiPsk,
      wifi_auth_mode_t::WIFI_AUTH_MAX => unreachable!("WIFI_AUTH_MAX"),
    }
  }
}

impl From<AuthMode> for wifi_auth_mode_t {
  fn from(auth_mode: AuthMode) -> Self {
    match auth_mode {
      AuthMode::Open => wifi_auth_mode_t::WIFI_AUTH_OPEN,
      AuthMode::Wep => wifi_auth_mode_t::WIFI_AUTH_WEP,
      AuthMode::WpaPsk => wifi_auth_mode_t::WIFI_AUTH_WPA_PSK,
      AuthMode::WpaWpa2Psk => wifi_auth_mode_t::WIFI_AUTH_WPA_WPA2_PSK,
      AuthMode::Wpa2Psk => wifi_auth_mode_t::WIFI_AUTH_WPA2_PSK,
      #[cfg(target_device = "esp32")]
      AuthMode::Wpa2Wpa3Psk => wifi_auth_mode_t::WIFI_AUTH_WPA2_WPA3_PSK,
      #[cfg(target_device = "esp32")]
      AuthMode::Wpa3Psk => wifi_auth_mode_t::WIFI_AUTH_WPA3_PSK,
      AuthMode::Wpa2Enterprise => wifi_auth_mode_t::WIFI_AUTH_WPA2_ENTERPRISE,
      AuthMode::WapiPsk => wifi_auth_mode_t::WIFI_AUTH_WAPI_PSK,
    }
  }
}
