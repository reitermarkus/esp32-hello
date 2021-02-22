use core::fmt;

use esp_idf_bindgen::{
  wifi_config_t,
  wifi_sta_config_t,
  wifi_scan_method_t,
  wifi_sort_method_t,
  wifi_scan_threshold_t,
};

use super::{AuthMode, Ssid, Password};

/// Scan method used when connecting to an access point.
#[derive(Debug, Clone, Copy)]
pub enum ScanMethod {
  Fast,
  Full,
}

impl Default for ScanMethod {
  fn default() -> Self {
    Self::Fast
  }
}

impl From<ScanMethod> for wifi_scan_method_t {
  fn from(scan_method: ScanMethod) -> Self {
    match scan_method {
      ScanMethod::Fast => wifi_scan_method_t::WIFI_FAST_SCAN,
      ScanMethod::Full => wifi_scan_method_t::WIFI_ALL_CHANNEL_SCAN,
    }
  }
}

/// Sort method for prioritization of access points to connect to.
#[derive(Debug, Clone, Copy)]
pub enum SortMethod {
  BySignal,
  BySecurity,
}

impl Default for SortMethod {
  fn default() -> Self {
    Self::BySignal
  }
}

impl From<SortMethod> for wifi_sort_method_t {
  fn from(sort_method: SortMethod) -> Self {
    match sort_method {
      SortMethod::BySignal => wifi_sort_method_t::WIFI_CONNECT_AP_BY_SIGNAL,
      SortMethod::BySecurity => wifi_sort_method_t::WIFI_CONNECT_AP_BY_SECURITY,
    }
  }
}

/// Scan threshold used when connecting to an access point.
#[derive(Debug, Clone, Copy)]
pub struct ScanThreshold {
  rssi: i8,
  auth_mode: AuthMode,
}

impl Default for ScanThreshold {
  fn default() -> Self {
    Self {
      rssi: -127,
      auth_mode: AuthMode::Open,
    }
  }
}

impl From<ScanThreshold> for wifi_scan_threshold_t {
  fn from(scan_threshold: ScanThreshold) -> Self {
    Self {
      rssi: scan_threshold.rssi,
      authmode: scan_threshold.auth_mode.into(),
    }
  }
}

/// Configuration for a station.
#[derive(Debug, Clone)]
pub struct StaConfig {
  ssid: Ssid,
  password: Password,
  scan_method: ScanMethod,
  bssid: Option<[u8; 6]>,
  channel: Option<u8>,
  listen_interval: Option<u16>,
  sort_method: SortMethod,
  threshold: Option<ScanThreshold>,
}

impl StaConfig {
  pub fn ssid(&self) -> &Ssid {
    &self.ssid
  }

  pub fn password(&self) -> &Password {
    &self.password
  }

  pub fn builder() -> StaConfigBuilder {
    StaConfigBuilder::default()
  }
}

impl From<&StaConfig> for wifi_config_t {
  fn from(sta_config: &StaConfig) -> Self {
    Self {
      sta: wifi_sta_config_t {
        ssid: sta_config.ssid.ssid,
        password: sta_config.password.password,
        scan_method: sta_config.scan_method.into(),
        bssid_set: sta_config.bssid.is_some(),
        bssid: sta_config.bssid.unwrap_or([0, 0, 0, 0, 0, 0]),
        channel: sta_config.channel.unwrap_or(0),
        listen_interval: sta_config.listen_interval.unwrap_or(0),
        sort_method: sta_config.sort_method.into(),
        threshold: sta_config.threshold.unwrap_or_default().into(),
        #[cfg(target_device = "esp32")]
        pmf_cfg: esp_idf_bindgen::wifi_pmf_config_t {
          capable: false,
          required: false,
        },
        _bitfield_1: wifi_sta_config_t::new_bitfield_1(0, 0, 0),
      }
    }
  }
}

/// Builder for [`StaConfig`](struct.StaConfig.html).
pub struct StaConfigBuilder {
  ssid: Option<Ssid>,
  password: Password,
  scan_method: ScanMethod,
  bssid: Option<[u8; 6]>,
  channel: Option<u8>,
  listen_interval: Option<u16>,
  sort_method: SortMethod,
  threshold: Option<ScanThreshold>,
}

impl fmt::Debug for StaConfigBuilder {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("StaConfigBuilder")
      .field("ssid", &self.ssid)
      .field("password", &"********")
      .field("scan_method", &self.scan_method)
      .field("bssid", &self.bssid)
      .field("channel", &self.channel)
      .field("listen_interval", &self.listen_interval)
      .field("sort_method", &self.sort_method)
      .field("threshold", &self.threshold)
      .finish()
  }
}

impl Default for StaConfigBuilder {
  fn default() -> Self {
    Self {
      ssid: None,
      password: Default::default(),
      scan_method: Default::default(),
      bssid: Default::default(),
      channel: Default::default(),
      listen_interval: Default::default(),
      sort_method: Default::default(),
      threshold: Default::default(),
    }
  }
}

impl StaConfigBuilder {
  pub fn ssid(&mut self, ssid: Ssid) -> &mut Self {
    self.ssid = Some(ssid);
    self
  }

  pub fn password(&mut self, password: Password) -> &mut Self {
    self.password = password;
    self
  }

  pub fn build(&self) -> StaConfig {
    StaConfig {
      ssid: self.ssid.clone().expect("missing SSID"),
      password: self.password.clone(),
      scan_method: self.scan_method,
      bssid: self.bssid,
      channel: self.channel,
      listen_interval: self.listen_interval,
      sort_method: self.sort_method,
      threshold: self.threshold,
    }
  }
}
