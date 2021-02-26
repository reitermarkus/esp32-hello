use core::cmp::Ordering;
use core::fmt;
use core::ops::Deref;
use core::ptr;
use std::str::{self, FromStr};

use super::WifiConfigError;

const SSID_MAX_LEN: usize = 32;

/// A WiFi SSID.
#[derive(Clone)]
pub struct Ssid {
  pub(crate) ssid: [u8; SSID_MAX_LEN],
  pub(crate) ssid_len: usize,
}

impl Deref for Ssid {
  type Target = str;

  fn deref(&self) -> &Self::Target {
    self.as_str()
  }
}

impl PartialEq for Ssid {
  fn eq(&self, other: &Self) -> bool {
    self.as_str() == other.as_str()
  }
}

impl Eq for Ssid {}

impl Ord for Ssid {
  fn cmp(&self, other: &Self) -> Ordering {
    self.as_str().cmp(&other.as_str())
  }
}

impl PartialOrd for Ssid {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ssid {
  #[inline]
  pub fn as_str(&self) -> &str {
    &unsafe { str::from_utf8_unchecked(&self.ssid[..self.ssid_len]) }
  }

  pub fn from_bytes(bytes: &[u8]) -> Result<Ssid, WifiConfigError> {
    let ssid_len = bytes.len();

    if ssid_len > SSID_MAX_LEN {
      return Err(WifiConfigError::TooLong(SSID_MAX_LEN, ssid_len))
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
  ///         does not exceed 32 bytes.
  pub unsafe fn from_bytes_unchecked(bytes: &[u8]) -> Ssid {
    let ssid_len = bytes.len();
    let mut ssid = [0; SSID_MAX_LEN];
    ptr::copy_nonoverlapping(bytes.as_ptr(), ssid.as_mut_ptr(), ssid_len);
    Self { ssid, ssid_len }
  }
}

impl FromStr for Ssid {
  type Err = WifiConfigError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Self::from_bytes(s.as_bytes())
  }
}

impl fmt::Debug for Ssid {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Ssid")
      .field("ssid", &self.as_str())
      .field("ssid_len", &self.ssid_len)
      .finish()
  }
}

impl fmt::Display for Ssid {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.as_str().fmt(f)
  }
}
