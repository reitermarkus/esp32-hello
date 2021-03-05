use core::fmt;
use core::mem;
use core::num::{NonZeroU8, NonZeroU16};

use esp_idf_bindgen::{wifi_config_t, wifi_ap_config_t};

use super::{AuthMode, Cipher, Ssid, Password};

/// Configuration for an access point.
#[derive(Clone)]
#[repr(transparent)]
pub struct ApConfig(pub(crate) wifi_config_t);

impl fmt::Debug for ApConfig {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("ApConfigBuilder")
      .field("ssid", &self.ssid())
      .field("password", &self.password())
      .field("channel", &self.channel())
      .field("auth_mode", &self.auth_mode())
      .field("max_connection", &self.max_connection())
      .field("ssid_hidden", &self.ssid_hidden())
      .field("beacon_interval", &self.beacon_interval())
      .field("pairwise_cipher", &self.pairwise_cipher())
      .finish()
  }
}

impl ApConfig {
  pub fn ssid(&self) -> &Ssid {
    unsafe { mem::transmute(&self.0.ap.ssid) }
  }

  pub fn password(&self) -> &Password {
    unsafe { mem::transmute(&self.0.ap.password) }
  }

  pub fn channel(&self) -> Option<NonZeroU8> {
    unsafe { mem::transmute(self.0.ap.channel) }
  }

  pub fn auth_mode(&self) -> AuthMode {
    AuthMode::from(unsafe { self.0.ap.authmode })
  }

  pub fn max_connection(&self) -> Option<NonZeroU8> {
    unsafe { mem::transmute(self.0.ap.max_connection) }
  }

  pub fn ssid_hidden(&self) -> bool {
    unsafe { mem::transmute(self.0.ap.ssid_hidden) }
  }

  pub fn beacon_interval(&self) -> Option<NonZeroU16> {
    unsafe { mem::transmute(self.0.ap.beacon_interval) }
  }

  pub fn pairwise_cipher(&self) -> Cipher {
    Cipher::from(unsafe { self.0.ap.pairwise_cipher })
  }

  pub fn builder() -> ApConfigBuilder {
    ApConfigBuilder::default()
  }
}

/// Builder for [`ApConfig`](struct.ApConfig.html).
pub struct ApConfigBuilder {
  ssid: Option<Ssid>,
  password: Password,
  channel: Option<NonZeroU8>,
  auth_mode: AuthMode,
  max_connection: Option<NonZeroU8>,
  ssid_hidden: bool,
  beacon_interval: Option<NonZeroU16>,
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
      channel: None,
      auth_mode: AuthMode::Open,
      max_connection: NonZeroU8::new(4),
      ssid_hidden: false,
      beacon_interval: NonZeroU16::new(100),
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
    let ssid = self.ssid.clone().expect("missing SSID");
    let ssid_len = ssid.len() as u8;

    ApConfig(wifi_config_t {
      ap: wifi_ap_config_t {
        ssid: ssid.0,
        ssid_len,
        password: self.password.clone().0,
        channel: unsafe { mem::transmute(self.channel) },
        authmode: self.auth_mode.into(),
        ssid_hidden: self.ssid_hidden as u8,
        max_connection: unsafe { mem::transmute(self.max_connection) },
        beacon_interval: unsafe { mem::transmute(self.beacon_interval) },
        pairwise_cipher: self.pairwise_cipher.into(),
      },
    })
  }
}
