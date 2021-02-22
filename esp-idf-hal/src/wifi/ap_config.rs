use core::fmt;

use esp_idf_bindgen::{wifi_config_t, wifi_ap_config_t};

use super::{AuthMode, Cipher, Ssid, Password};

/// Configuration for an access point.
#[derive(Clone)]
pub struct ApConfig {
  ssid: Ssid,
  password: Password,
  channel: u8,
  auth_mode: AuthMode,
  max_connection: u8,
  ssid_hidden: bool,
  beacon_interval: u16,
  pairwise_cipher: Cipher,
}

impl fmt::Debug for ApConfig {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("ApConfigBuilder")
      .field("ssid", &self.ssid)
      .field("password", &"********")
      .field("channel", &self.channel)
      .field("auth_mode", &self.auth_mode)
      .field("max_connection", &self.max_connection)
      .field("ssid_hidden", &self.ssid_hidden)
      .field("beacon_interval", &self.beacon_interval)
      .field("pairwise_cipher", &self.pairwise_cipher)
      .finish()
  }
}

impl ApConfig {
  pub fn ssid(&self) -> &Ssid {
    &self.ssid
  }

  pub fn password(&self) -> &Password {
    &self.password
  }

  pub fn builder() -> ApConfigBuilder {
    ApConfigBuilder::default()
  }
}

impl From<&ApConfig> for wifi_config_t {
  fn from(ap_config: &ApConfig) -> Self {
    Self {
      ap: wifi_ap_config_t {
        ssid: ap_config.ssid.ssid,
        ssid_len: ap_config.ssid.ssid_len as u8,
        password: ap_config.password.password,
        channel: ap_config.channel,
        authmode: ap_config.auth_mode.into(),
        ssid_hidden: ap_config.ssid_hidden as u8,
        max_connection: ap_config.max_connection,
        beacon_interval: ap_config.beacon_interval,
        pairwise_cipher: ap_config.pairwise_cipher.into(),
      },
    }
  }
}

/// Builder for [`ApConfig`](struct.ApConfig.html).
pub struct ApConfigBuilder {
  ssid: Option<Ssid>,
  password: Password,
  channel: u8,
  auth_mode: AuthMode,
  max_connection: u8,
  ssid_hidden: bool,
  beacon_interval: u16,
  pairwise_cipher: Cipher,
}

impl fmt::Debug for ApConfigBuilder {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("ApConfigBuilder")
      .field("ssid", &self.ssid)
      .field("password", &"********")
      .field("channel", &self.channel)
      .field("auth_mode", &self.auth_mode)
      .field("max_connection", &self.max_connection)
      .field("ssid_hidden", &self.ssid_hidden)
      .field("beacon_interval", &self.beacon_interval)
      .field("pairwise_cipher", &self.pairwise_cipher)
      .finish()
  }
}

impl Default for ApConfigBuilder {
  fn default() -> Self {
    Self {
      ssid: None,
      password: Default::default(),
      channel: 0,
      auth_mode: AuthMode::Open,
      max_connection: 4,
      ssid_hidden: false,
      beacon_interval: 100,
      pairwise_cipher: Cipher::None,
    }
  }
}

impl ApConfigBuilder {
  pub fn ssid(&mut self, ssid: Ssid) -> &mut Self {
    self.ssid = Some(ssid);
    self
  }

  pub fn password(&mut self, password: Password) -> &mut Self {
    self.password = password;
    self
  }

  pub fn build(&self) -> ApConfig {
    ApConfig {
      ssid: self.ssid.clone().expect("missing SSID"),
      password: self.password.clone(),
      channel: self.channel,
      auth_mode: self.auth_mode,
      max_connection: self.max_connection,
      ssid_hidden: self.ssid_hidden,
      beacon_interval: self.beacon_interval,
      pairwise_cipher: self.pairwise_cipher,
    }
  }
}
