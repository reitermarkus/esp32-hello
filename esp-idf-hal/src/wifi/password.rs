use core::fmt;
use core::ptr;
use std::str::{self, FromStr};

use super::WifiConfigError;

const PASSWORD_MAX_LEN: usize = 64;

/// A WiFi password.
#[derive(Clone)]
#[repr(transparent)]
pub struct Password(pub(crate) [u8; PASSWORD_MAX_LEN]);

impl Password {
  fn len(&self) -> usize {
    memchr::memchr(0, &self.0).unwrap_or(PASSWORD_MAX_LEN)
  }

  pub fn as_str(&self) -> &str {
    &unsafe { str::from_utf8_unchecked(&self.0[..self.len()]) }
  }

  pub fn from_bytes(bytes: &[u8]) -> Result<Password, WifiConfigError> {
    let ssid_len = bytes.len();

    if ssid_len > PASSWORD_MAX_LEN {
      return Err(WifiConfigError::TooLong(PASSWORD_MAX_LEN, ssid_len))
    }

    if let Err(utf8_error) = str::from_utf8(bytes) {
      return Err(WifiConfigError::Utf8Error(utf8_error))
    }

    if let Some(pos) = memchr::memchr(0, bytes) {
      return Err(WifiConfigError::InteriorNul(pos))
    }

    Ok(unsafe { Self::from_bytes_unchecked(bytes) })
  }

  /// SAFTEY: The caller has to ensure that `bytes` does not contain a `NUL` byte and
  ///         does not exceed 64 bytes.
  pub unsafe fn from_bytes_unchecked(bytes: &[u8]) -> Password {
    let password_len = bytes.len();
    let mut password = Self([0; PASSWORD_MAX_LEN]);
    ptr::copy_nonoverlapping(bytes.as_ptr(), password.0.as_mut_ptr(), password_len);
    password
  }
}

impl Default for Password {
  #[inline(always)]
  fn default() -> Self {
    Self([0; PASSWORD_MAX_LEN])
  }
}

impl FromStr for Password {
  type Err = WifiConfigError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Self::from_bytes(s.as_bytes())
  }
}

impl fmt::Debug for Password {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Password")
      .field("password", &self.as_str())
      .finish()
  }
}

impl fmt::Display for Password {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    #[cfg(debug)]
    return self.as_str().fmt(f);

    #[cfg(not(debug))]
    return "********".fmt(f);
  }
}
