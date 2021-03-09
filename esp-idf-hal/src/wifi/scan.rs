use core::cmp;
use core::future::Future;
use core::marker::PhantomData;
use core::mem::{self, MaybeUninit};
use core::pin::Pin;
use core::ptr;
use core::task::{Poll, Context, Waker};
use std::time::Duration;

use esp_idf_bindgen::{
  esp_wifi_scan_start,
  esp_wifi_scan_get_ap_num,
  esp_wifi_scan_get_ap_records,
  wifi_ap_record_t,
  wifi_scan_config_t,
  wifi_scan_time_t,
  wifi_active_scan_time_t,
  wifi_scan_type_t,
};
use macaddr::MacAddr6;
use pin_project::pin_project;

use super::*;

/// Scan type used for scanning nearby WiFi networks.
///
/// For an explanation of the two types, refer to https://www.wi-fi.org/knowledge-center/faq/what-are-passive-and-active-scanning.
///
/// All durations must be between `1` and `u32::max_value()` milliseconds. A duration of `0` means that the default duration will be used.
#[derive(Debug, Clone)]
pub enum ScanType {
  /// Active scanning with a minimum duration of `min` and a maximum duration of `max` per channel.
  Active { min: Duration, max: Duration },
  /// Passive scanning with a maximum duration of `max` per channel.
  Passive { max: Duration },
}

impl Default for ScanType {
  fn default() -> Self {
    Self::Active { min: Duration::from_millis(0), max: Duration::from_millis(0) }
  }
}

/// Configuration used for scanning nearby WiFi networks.
#[derive(Default, Debug, Clone)]
pub struct ScanConfig {
  ssid: Option<Ssid>,
  bssid: Option<MacAddr6>,
  channel: u8,
  show_hidden: bool,
  scan_type: ScanType,
}

impl ScanConfig {
  pub fn builder() -> ScanConfigBuilder {
    ScanConfigBuilder {
      ssid: None,
      bssid: None,
      channel: 0,
      show_hidden: false,
      scan_type: Default::default(),
    }
  }
}

/// Builder for [`ScanConfig`](struct.ScanConfig.html).
#[derive(Debug, Clone)]
pub struct ScanConfigBuilder {
  ssid: Option<Ssid>,
  bssid: Option<MacAddr6>,
  channel: u8,
  show_hidden: bool,
  scan_type: ScanType,
}

impl ScanConfigBuilder {
  pub fn ssid(mut self, ssid: impl Into<Option<Ssid>>) -> ScanConfigBuilder {
    self.ssid = ssid.into();
    self
  }

  pub fn bssid(mut self, bssid: impl Into<Option<MacAddr6>>) -> ScanConfigBuilder {
    self.bssid = bssid.into();
    self
  }


  pub fn channel(mut self, channel: u8) -> ScanConfigBuilder {
    self.channel = channel;
    self
  }

  pub fn show_hidden(mut self, show_hidden: bool) -> ScanConfigBuilder {
    self.show_hidden = show_hidden;
    self
  }

  pub fn scan_type(mut self, scan_type: ScanType) -> ScanConfigBuilder {
    #[cfg(debug)]
    if let ScanType::Active { min, max } = scan_type {
      if max != Duration::default() {
        assert!(min <= max);
      }
    }
    self.scan_type = scan_type;
    self
  }

  pub fn build(self) -> ScanConfig {
    let Self { ssid, bssid, channel, show_hidden, scan_type } = self;
    ScanConfig { ssid, bssid, channel, show_hidden, scan_type }
  }
}

/// An access point record returned by a [`ScanFuture`](struct.ScanFuture.html).
#[derive(Debug, Clone)]
pub struct ApRecord {
  ssid: Ssid,
  bssid: MacAddr6,
  auth_mode: AuthMode,
}

impl ApRecord {
  pub fn ssid(&self) -> &Ssid {
    &self.ssid
  }

  pub fn bssid(&self) -> &MacAddr6 {
    &self.bssid
  }

  pub fn auth_mode(&self) -> &AuthMode {
    &self.auth_mode
  }
}

#[derive(Debug)]
enum ScanFutureState {
  Starting(wifi_scan_config_t, StaMode, Option<Waker>),
  Done,
}

/// A future representing a scan of nearby WiFi networks.
#[must_use = "futures do nothing unless polled"]
#[pin_project]
#[derive(Debug)]
pub struct ScanFuture<'w> {
  handler: Option<EventHandler>,
  state: ScanFutureState,
  wifi: &'w mut Wifi,
}

impl<'w> ScanFuture<'w> {
  #[inline]
  pub(crate) fn new(wifi: &'w mut Wifi, config: &ScanConfig) -> Self {
    let duration_as_millis_rounded = |dur: Duration| {
      let nanos = dur.as_nanos();

      if nanos == 0 {
        0
      } else {
        cmp::min(u32::max_value() as u128, cmp::max(1_000_000, nanos) / 1_000_000) as u32
      }
    };

    let (scan_type, scan_time) = match config.scan_type {
      ScanType::Active { min, max } => (
        wifi_scan_type_t::WIFI_SCAN_TYPE_ACTIVE,
        wifi_scan_time_t {
          active: wifi_active_scan_time_t {
            min: duration_as_millis_rounded(min),
            max: duration_as_millis_rounded(max),
          },
          #[cfg(target_device = "esp32")]
          passive: 0,
        },
      ),
      ScanType::Passive { max } => (
        wifi_scan_type_t::WIFI_SCAN_TYPE_PASSIVE,
        wifi_scan_time_t {
          #[cfg(target_device = "esp32")]
          active: wifi_active_scan_time_t { min: 0, max: 0 },
          passive: duration_as_millis_rounded(max),
        },
      )
    };

    let config = wifi_scan_config_t {
      ssid: config.ssid.as_ref().map_or_else(ptr::null_mut, |ssid| ssid.0.as_ptr() as *mut _),
      bssid: config.bssid.as_ref().map_or_else(ptr::null_mut, |bssid| bssid as *const _ as *mut _),
      channel: config.channel,
      show_hidden: config.show_hidden,
      scan_type,
      scan_time,
    };

    Self {
      handler: None,
      state: ScanFutureState::Starting(config, StaMode::enter(), None),
      wifi,
    }
  }
}

impl Future for ScanFuture<'_> {
  type Output = Result<Vec<ApRecord>, WifiError>;

  #[cfg(target_device = "esp8266")]
  fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
    Poll::Pending
  }

  #[cfg(target_device = "esp32")]
  fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
    match self.state {
      ScanFutureState::Starting(config, _, ref mut waker) => {
        waker.replace(cx.waker().clone());

        esp_ok!(esp_wifi_start())?;

        let arg = &mut *self as *mut _;
        self.handler.replace(EventHandler::register(
          unsafe { WIFI_EVENT }, wifi_event_t::WIFI_EVENT_SCAN_DONE as _, wifi_scan_done_handler, arg as _
        )?);

        if let Err(err) = esp_ok!(esp_wifi_scan_start(&config, false)) {
          return Poll::Ready(Err(err.into()))
        };

        Poll::Pending
      },
      ScanFutureState::Done => {
        Poll::Ready(Ok(get_ap_records()?))
      }
    }
  }
}

#[inline]
fn get_ap_records() -> Result<Vec<ApRecord>, EspError> {
  let mut ap_num = 0;
  esp_ok!(esp_wifi_scan_get_ap_num(&mut ap_num))?;

  let mut aps: Vec<MaybeUninit<wifi_ap_record_t>> = vec![MaybeUninit::uninit(); ap_num as usize];
  esp_ok!(esp_wifi_scan_get_ap_records(&mut ap_num as _, aps.as_mut_ptr() as *mut wifi_ap_record_t))?;

  Ok(aps.into_iter().map(|ap| {
    // SAFETY: At this point we have asserted that `esp_wifi_scan_get_ap_records` returned `ESP_OK`.
    let ap = unsafe { ap.assume_init() };

    // SAFETY: We made sure that the SSID does not contain a `NUL` byte and
    //         `ap.ssid` is at most 32 bytes long.
    let ssid = unsafe {
      let ssid_len = memchr::memchr(0, &ap.ssid).unwrap_or(ap.ssid.len());
      Ssid::from_bytes_unchecked(&ap.ssid[..ssid_len])
    };

    let bssid = MacAddr6::from(ap.bssid);
    let auth_mode = ap.authmode.into();

    ApRecord { ssid, bssid, auth_mode }
  }).collect())
}

#[cfg(target_device = "esp32")]
extern "C" fn wifi_scan_done_handler(
  event_handler_arg: *mut libc::c_void,
  _event_base: esp_idf_bindgen::esp_event_base_t,
  _event_id: i32,
  _event_data: *mut libc::c_void,
) {
  // SAFETY: `wifi_scan_done_handler` is only registered while the `event_handler_arg` is
  //         pointing to a `ScanFuture` contained in a `Pin`.
  let mut f = unsafe { Pin::new_unchecked(&mut *(event_handler_arg as *mut ScanFuture)) };
  if let ScanFutureState::Starting(_, _, Some(waker)) = mem::replace(&mut f.state, ScanFutureState::Done) {
    waker.wake();
  }
}
