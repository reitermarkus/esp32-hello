use esp_idf_bindgen::wifi_cipher_type_t;

/// A WiFi cipher type.
#[derive(Debug, Clone, Copy)]
pub enum Cipher {
  None,
  Wep40,      /// WEP40
  Wep104,     /// WEP104
  Tkip,       /// TKIP
  Ccmp,       /// CCMP
  TkipCcmp,   /// TKIP and CCMP
  AesCmac128, /// AES-CMAC-128
  Sms4,       /// SMS4
  Unknown,
}

impl From<Cipher> for wifi_cipher_type_t {
  fn from(cipher: Cipher) -> Self {
    match cipher {
      Cipher::None         => wifi_cipher_type_t::WIFI_CIPHER_TYPE_NONE,
      Cipher::Wep40        => wifi_cipher_type_t::WIFI_CIPHER_TYPE_WEP40,
      Cipher::Wep104       => wifi_cipher_type_t::WIFI_CIPHER_TYPE_WEP104,
      Cipher::Tkip         => wifi_cipher_type_t::WIFI_CIPHER_TYPE_TKIP,
      Cipher::Ccmp         => wifi_cipher_type_t::WIFI_CIPHER_TYPE_CCMP,
      Cipher::TkipCcmp     => wifi_cipher_type_t::WIFI_CIPHER_TYPE_TKIP_CCMP,
      Cipher::AesCmac128   => wifi_cipher_type_t::WIFI_CIPHER_TYPE_AES_CMAC128,
      Cipher::Sms4         => wifi_cipher_type_t::WIFI_CIPHER_TYPE_SMS4,
      Cipher::Unknown      => wifi_cipher_type_t::WIFI_CIPHER_TYPE_UNKNOWN,
    }
  }
}

impl From<wifi_cipher_type_t> for Cipher {
  fn from(cipher: wifi_cipher_type_t) -> Self {
    match cipher {
      wifi_cipher_type_t::WIFI_CIPHER_TYPE_NONE        => Cipher::None,
      wifi_cipher_type_t::WIFI_CIPHER_TYPE_WEP40       => Cipher::Wep40,
      wifi_cipher_type_t::WIFI_CIPHER_TYPE_WEP104      => Cipher::Wep104,
      wifi_cipher_type_t::WIFI_CIPHER_TYPE_TKIP        => Cipher::Tkip,
      wifi_cipher_type_t::WIFI_CIPHER_TYPE_CCMP        => Cipher::Ccmp,
      wifi_cipher_type_t::WIFI_CIPHER_TYPE_TKIP_CCMP   => Cipher::TkipCcmp,
      wifi_cipher_type_t::WIFI_CIPHER_TYPE_AES_CMAC128 => Cipher::AesCmac128,
      wifi_cipher_type_t::WIFI_CIPHER_TYPE_SMS4        => Cipher::Sms4,
      wifi_cipher_type_t::WIFI_CIPHER_TYPE_UNKNOWN     => Cipher::Unknown,
    }
  }
}
