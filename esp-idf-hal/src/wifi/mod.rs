use core::mem::{self, transmute, MaybeUninit};
use core::num::NonZeroU8;
use std::str::Utf8Error;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering::SeqCst};
use core::task::{Poll, Context, Waker};
use core::pin::Pin;

use core::fmt;
use macaddr::MacAddr6;
use pin_project::pin_project;

use crate::{EspError, nvs::NonVolatileStorage, interface::{Interface, IpInfo}};

use esp_idf_bindgen::*;

mod sta_config;
pub use sta_config::*;

mod ap_config;
pub use ap_config::*;

mod scan;
pub use scan::*;

mod ssid;
pub use ssid::Ssid;

mod password;
pub use password::Password;

mod event_handler;
use event_handler::EventHandler;

mod auth_mode;
pub use auth_mode::AuthMode;

mod cipher;
pub use cipher::Cipher;

/// Error returned by [`Ssid::from_bytes`](struct.Ssid.html#method.from_bytes)
/// and [`Password::from_bytes`](struct.Password.html#method.from_bytes).
#[derive(Debug)]
pub enum WifiConfigError {
  /// SSID or password contains interior `NUL`-bytes.
  InteriorNul(usize),
  /// SSID or password is too long.
  TooLong(usize, usize),
  /// SSID or password is not valid UTF-8.
  Utf8Error(Utf8Error),
}

impl fmt::Display for WifiConfigError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::InteriorNul(pos) => write!(f, "data provided contains an interior nul byte at pos {}", pos),
      Self::TooLong(max, actual) => write!(f, "data provided is {} bytes long, but maximum is {} bytes", max, actual),
      Self::Utf8Error(utf8_error) => utf8_error.fmt(f),
    }
  }
}

#[derive(Debug)]
pub struct Ap {
  mode: ApMode,
}

impl Ap {
  pub fn config(&self) -> ApConfig {
    let config = MaybeUninit::<ApConfig>::uninit();
    esp_ok!(esp_wifi_get_config(wifi_interface_t::WIFI_IF_AP, config.as_ptr() as *mut _)).unwrap();
    unsafe { config.assume_init() }
  }

  pub fn ip_info(&self) -> IpInfo {
    Interface::Ap.ip_info()
  }
}

#[derive(Debug)]
pub struct Sta {
  mode: StaMode,
}

impl Sta {
  pub fn config(&self) -> StaConfig {
    let config = MaybeUninit::<StaConfig>::uninit();
    esp_ok!(esp_wifi_get_config(wifi_interface_t::WIFI_IF_STA, config.as_ptr() as *mut _)).unwrap();
    unsafe { config.assume_init() }
  }

  pub fn ip_info(&self) -> IpInfo {
    Interface::Sta.ip_info()
  }
}

#[derive(Debug)]
enum WifiInner {
  None,
  Ap(Ap),
  Sta(Sta),
  ApSta(Ap, Sta),
}

impl Default for WifiInner {
  fn default() -> Self {
    Self::None
  }
}

/// An instance of the WiFi peripheral.
#[must_use = "WiFi will be stopped and deinitialized immediately. Drop it explicitly after you are done using it or create a named binding."]
#[derive(Debug)]
pub struct Wifi {
  inner: WifiInner,
}

#[cfg(target_device = "esp8266")]
fn initialize_network_interface() {
  unsafe { tcpip_adapter_init() };
}

#[cfg(target_device = "esp32")]
fn initialize_network_interface() {
  static NETIF_STATE: AtomicU8 = AtomicU8::new(0);

  loop {
    match NETIF_STATE.compare_exchange(0, 1, SeqCst, SeqCst) {
      Ok(0) => {
        esp_ok!(esp_netif_init()).expect("failed to initialize network interface");
        NETIF_STATE.store(2, SeqCst);
        return;
      },
      Err(1) => continue,
      _ => return,
    }
  }
}

fn event_loop_create_default() {
  static EVENT_LOOP_STATE: AtomicU8 = AtomicU8::new(0);

  loop {
    match EVENT_LOOP_STATE.compare_exchange(0, 1, SeqCst, SeqCst) {
      Ok(0) => {
        esp_ok!(esp_event_loop_create_default()).expect("failed to initialize default event loop");
        EVENT_LOOP_STATE.store(2, SeqCst);
        return;
      },
      Err(1) => continue,
      _ => return,
    }
  }
}

static AP_COUNT: AtomicU8 = AtomicU8::new(0);
static STA_COUNT: AtomicU8 = AtomicU8::new(0);

fn get_mode() -> Result<wifi_mode_t, EspError> {
  let mut mode = wifi_mode_t::WIFI_MODE_NULL;
  esp_ok!(esp_wifi_get_mode(&mut mode))?;
  eprintln!("esp_wifi_get_mode: {:?} ({} APs, {} STAs)", mode, AP_COUNT.load(SeqCst), STA_COUNT.load(SeqCst));
  Ok(mode)
}

#[derive(Debug)]
struct ApMode;

impl ApMode {
  pub fn enter() -> Self {
    enter_ap_mode().unwrap();
    Self
  }
}

impl Drop for ApMode {
  fn drop(&mut self) {
    let _ = leave_ap_mode();
  }
}

fn enter_ap_mode() -> Result<(), EspError> {
  let count = AP_COUNT.fetch_add(1, SeqCst);
  eprintln!("AP ENTER {}", count);
  if count > 0 {
    return Ok(())
  }

  let current_mode = get_mode()?;

  let new_mode = match current_mode {
    wifi_mode_t::WIFI_MODE_AP | wifi_mode_t::WIFI_MODE_APSTA => return Ok(()),
    wifi_mode_t::WIFI_MODE_NULL => wifi_mode_t::WIFI_MODE_AP,
    wifi_mode_t::WIFI_MODE_STA => wifi_mode_t::WIFI_MODE_APSTA,
    _ => unreachable!(),
  };

  eprintln!("{:?} -> {:?}", current_mode, new_mode);
  esp_ok!(esp_wifi_set_mode(new_mode))
}

fn leave_ap_mode() -> Result<(), EspError> {
  let count = AP_COUNT.fetch_sub(1, SeqCst);
  eprintln!("AP LEAVE {}", count);
  if count != 1 {
    return Ok(())
  }

  let current_mode = get_mode()?;

  let new_mode = match current_mode {
    wifi_mode_t::WIFI_MODE_AP => wifi_mode_t::WIFI_MODE_NULL,
    wifi_mode_t::WIFI_MODE_APSTA => wifi_mode_t::WIFI_MODE_STA,
    _ => unreachable!(),
  };

  eprintln!("{:?} -> {:?}", current_mode, new_mode);
  esp_ok!(esp_wifi_set_mode(new_mode))?;
  get_mode()?;

  if new_mode == wifi_mode_t::WIFI_MODE_NULL {
    // esp_ok!(esp_wifi_restore())?;
    esp_ok!(esp_wifi_stop())?;
  }

  Ok(())
}

#[derive(Debug)]
struct StaMode;

impl StaMode {
  pub fn enter() -> Self {
    enter_sta_mode().unwrap();
    Self
  }
}

impl Drop for StaMode {
  fn drop(&mut self) {
    let _ = leave_sta_mode();
  }
}

fn enter_sta_mode() -> Result<(), EspError> {
  let count = STA_COUNT.fetch_add(1, SeqCst);
  eprintln!("STA ENTER {}", count);
  if count > 0 {
    return Ok(())
  }

  let current_mode = get_mode()?;

  let new_mode = match current_mode {
    wifi_mode_t::WIFI_MODE_STA | wifi_mode_t::WIFI_MODE_APSTA => return Ok(()),
    wifi_mode_t::WIFI_MODE_NULL => wifi_mode_t::WIFI_MODE_STA,
    wifi_mode_t::WIFI_MODE_AP => wifi_mode_t::WIFI_MODE_APSTA,
    _ => unreachable!(),
  };

  eprintln!("{:?} -> {:?}", current_mode, new_mode);
  esp_ok!(esp_wifi_set_mode(new_mode))
}

fn leave_sta_mode() -> Result<(), EspError> {
  let count = STA_COUNT.fetch_sub(1, SeqCst);
  eprintln!("STA LEAVE {}", count);
  if count != 1 {
    return Ok(())
  }

  let current_mode = get_mode()?;

  let new_mode = match current_mode {
    wifi_mode_t::WIFI_MODE_STA => wifi_mode_t::WIFI_MODE_NULL,
    wifi_mode_t::WIFI_MODE_APSTA => wifi_mode_t::WIFI_MODE_AP,
    _ => unreachable!(),
  };

  eprintln!("{:?} -> {:?}", current_mode, new_mode);
  esp_ok!(esp_wifi_set_mode(new_mode))?;
  get_mode()?;

  if new_mode == wifi_mode_t::WIFI_MODE_NULL {
    esp_ok!(esp_wifi_stop())?;
  }

  Ok(())
}

static WIFI_ACTIVE: AtomicBool = AtomicBool::new(false);

impl Wifi {
  /// Take the WiFi peripheral if it is not already in use.
  pub fn take() -> Option<Wifi> {
    if WIFI_ACTIVE.compare_exchange(false, true, SeqCst, SeqCst) == Err(true) {
      None
    } else {
      initialize_network_interface();

      event_loop_create_default();

      NonVolatileStorage::init_default().expect("failed to initialize default NVS partition");
      let config = wifi_init_config_t::default();
      esp_ok!(esp_wifi_init(&config)).expect("failed to initialize WiFi with default configuration");
      esp_ok!(esp_wifi_set_mode(wifi_mode_t::WIFI_MODE_NULL)).unwrap();

      Some(Wifi { inner: WifiInner::None })
    }
  }

  /// Start an access point using the specified [`ApConfig`](struct.ApConfig.html).
  pub fn start_ap(&mut self, mut config: ApConfig) -> Result<(), WifiError> {
    eprintln!("Starting AP");

    let interface = Interface::Ap;
    interface.init();

    let ap_mode = ApMode::enter();

    if let Err(err) = esp_ok!(esp_wifi_set_config(wifi_interface_t::WIFI_IF_AP, &mut config.0)).and_then(|_| {
      esp_ok!(esp_wifi_start())
    }) {
      return Err(err.into());
    }

    let inner = match mem::take(&mut self.inner) {
      WifiInner::Sta(sta) => WifiInner::ApSta(Ap { mode: ap_mode }, sta),
      _ => WifiInner::Ap(Ap { mode: ap_mode }),
    };
    self.inner = inner;

    Ok(())
  }

  /// Connect to a WiFi network using the specified [`StaConfig`](struct.StaConfig.html).
  pub fn connect_sta<'w>(&'w mut self, mut config: StaConfig) -> ConnectFuture<'w> {
    eprintln!("Starting STA connection");

    Interface::Sta.init();

    let sta_mode = Some(StaMode::enter());

    let state = if let Err(err) = esp_ok!(esp_wifi_set_config(wifi_interface_t::WIFI_IF_STA, &mut config.0)) {
        ConnectFutureState::Failed(err.into())
    } else {
      ConnectFutureState::Starting
    };

    ConnectFuture { waker: None, mode: sta_mode, state, handlers: None, wifi: self }
  }
}

impl Wifi {
  /// Scan nearby WiFi networks using the specified [`ScanConfig`](struct.ScanConfig.html).
  pub fn scan(&mut self, scan_config: &ScanConfig) -> ScanFuture<'_> {
    ScanFuture::new(self, scan_config)
  }

  pub fn as_sta(&self) -> Option<&Sta> {
    match &self.inner {
      WifiInner::Sta(sta) => Some(sta),
      WifiInner::ApSta(_, sta) => Some(sta),
      _ => None,
    }
  }

  pub fn as_ap(&self) -> Option<&Ap> {
    match &self.inner {
      WifiInner::Ap(ap) => Some(ap),
      WifiInner::ApSta(ap, _) => Some(ap),
      _ => None,
    }
  }
}

impl Drop for Wifi {
  /// Stops a running WiFi instance and deinitializes it, making it available again
  /// by calling [`Wifi::take()`](struct.Wifi.html#method.take).
  fn drop(&mut self) {
    if !matches!(self.inner, WifiInner::None) {
      unsafe { esp_wifi_stop() };
    }

    let _ = esp_ok!(esp_wifi_deinit());
    NonVolatileStorage::deinit_default();

    WIFI_ACTIVE.store(false, SeqCst);
  }
}

impl Wifi {
  /// Stop a running WiFi in station mode.
  pub fn stop_sta(&mut self) -> &mut Wifi {
    eprintln!("Stopping STA");

    let inner = match mem::take(&mut self.inner) {
      WifiInner::ApSta(ap, _) => WifiInner::Ap(ap),
      _ => WifiInner::None,
    };
    self.inner = inner;

    self
  }

  /// Stop a running WiFi access point.
  pub fn stop_ap(&mut self) -> &mut Wifi {
    eprintln!("Stopping AP");

    let inner = match mem::take(&mut self.inner) {
      WifiInner::ApSta(_, sta) => WifiInner::Sta(sta),
      _ => WifiInner::None,
    };
    self.inner = inner;

    self
  }
}

#[derive(Debug)]
enum ConnectFutureState {
  Failed(WifiError),
  Starting,
  ConnectedWithoutIp { ssid: Ssid, bssid: MacAddr6, channel: Option<NonZeroU8>, auth_mode: AuthMode },
  Connected { ip_info: IpInfo, ssid: Ssid, bssid: MacAddr6, channel: Option<NonZeroU8>, auth_mode: AuthMode },
}

/// A future representing an ongoing connection to an access point.
#[must_use = "futures do nothing unless polled"]
#[pin_project]
#[derive(Debug)]
pub struct ConnectFuture<'w> {
  waker: Option<Waker>,
  mode: Option<StaMode>,
  state: ConnectFutureState,
  handlers: Option<[EventHandler; 4]>,
  wifi: &'w mut Wifi,
}

/// The type returned when a [`ConnectFuture`](struct.ConnectFuture.html) succeeds.
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
  ip_info: IpInfo,
  ssid: Ssid,
  bssid: MacAddr6,
  channel: NonZeroU8,
  auth_mode: AuthMode,
}

impl ConnectionInfo {
  #[inline]
  pub fn ip_info(&self) -> &IpInfo {
    &self.ip_info
  }

  #[inline]
  pub fn ssid(&self) -> &Ssid {
    &self.ssid
  }

  #[inline]
  pub fn bssid(&self) -> &MacAddr6 {
    &self.bssid
  }

  #[inline]
  pub fn channel(&self) -> NonZeroU8 {
    self.channel
  }

  #[inline]
  pub fn auth_mode(&self) -> AuthMode {
    self.auth_mode
  }
}

/// The error type returned when a [`ConnectFuture`](struct.ConnectFuture.html) fails.
#[derive(Debug, Clone)]
pub struct ConnectionError {
  ssid: Ssid,
  bssid: MacAddr6,
  reason: wifi_err_reason_t,
}

impl fmt::Display for ConnectionError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Error connecting to {} ({}): {:?}", self.ssid, self.bssid, self.reason)
  }
}

/// The error type for operations on a [`Wifi`](struct.Wifi.html) instance.
#[derive(Debug, Clone)]
pub enum WifiError {
  /// An internal error not directly related to WiFi.
  Internal(EspError),
  /// A connection error returned when a [`ConnectFuture`](struct.ConnectFuture.html) fails.
  ConnectionError(ConnectionError),
}

impl From<EspError> for WifiError {
  fn from(esp_error: EspError) -> Self {
    Self::Internal(esp_error)
  }
}

impl fmt::Display for WifiError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Internal(esp_error) => esp_error.fmt(f),
      Self::ConnectionError(error) => error.fmt(f),
    }
  }
}

impl core::future::Future for ConnectFuture<'_> {
  type Output = Result<ConnectionInfo, WifiError>;

  #[cfg(target_device = "esp8266")]
  fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
    Poll::Pending
  }

  #[cfg(target_device = "esp32")]
  fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
    match self.state {
      ConnectFutureState::Starting => {
        self.waker.replace(cx.waker().clone());

        let register_handlers = |arg: *mut ConnectFuture| -> Result<[EventHandler; 4], EspError> {
          Ok([
            EventHandler::register(unsafe { WIFI_EVENT }, wifi_event_t::WIFI_EVENT_STA_START as _, wifi_sta_handler, arg as _)?,
            EventHandler::register(unsafe { WIFI_EVENT }, wifi_event_t::WIFI_EVENT_STA_CONNECTED as _, wifi_sta_handler, arg as _)?,
            EventHandler::register(unsafe { WIFI_EVENT }, wifi_event_t::WIFI_EVENT_STA_DISCONNECTED as _, wifi_sta_handler, arg as _)?,
            EventHandler::register(unsafe { IP_EVENT }, ip_event_t::IP_EVENT_STA_GOT_IP as _, wifi_sta_handler, arg as _)?,
          ])
        };

        match register_handlers(&mut *self as *mut _) {
          Ok(handlers) => { self.handlers.replace(handlers); },
          Err(err) => {
            return Poll::Ready(Err(WifiError::from(err)));
          },
        }

        if let Err(err) = esp_ok!(esp_wifi_start()) {
          return Poll::Ready(Err(WifiError::from(err)));
        }

        Poll::Pending
      },
      ConnectFutureState::Failed(ref err) => {
        return Poll::Ready(Err(WifiError::from(err.clone())));
      },
      ConnectFutureState::ConnectedWithoutIp { .. } => {
        Poll::Pending
      },
      ConnectFutureState::Connected {
        ip_info,
        ssid,
        bssid,
        channel,
        auth_mode,
      } => {
        eprintln!("Ended STA connection");

        let connection_info = ConnectionInfo {
          ip_info,
          ssid,
          bssid,
          channel: channel.unwrap(),
          auth_mode,
        };

        let mode = self.mode.take().unwrap();
        let inner = match mem::take(&mut self.wifi.inner) {
          WifiInner::Ap(ap) => WifiInner::ApSta(ap, Sta { mode }),
          _ => WifiInner::Sta(Sta { mode }),
        };
        self.wifi.inner = inner;

        Poll::Ready(Ok(connection_info))
      },
    }
  }
}

#[cfg(target_device = "esp32")]
extern "C" fn wifi_sta_handler(
  event_handler_arg: *mut libc::c_void,
  event_base: esp_event_base_t,
  event_id: i32,
  event_data: *mut libc::c_void,
) {
  // SAFETY: `wifi_sta_handler` is only registered while the `event_handler_arg` is
  //         pointing to a `ConnectFuture` contained in a `Pin`.
  let mut f = unsafe { Pin::new_unchecked(&mut *(event_handler_arg as *mut ConnectFuture)) };

  if event_base == unsafe { WIFI_EVENT } {
    let event_id: wifi_event_t = unsafe { transmute(event_id) };

    eprintln!("WIFI_EVENT: {:?}", event_id);

    match event_id {
      wifi_event_t::WIFI_EVENT_STA_START => {
        if let Err(err) = esp_ok!(esp_wifi_connect()) {
          f.state = ConnectFutureState::Failed(err.into());
          f.waker.as_ref().map(|w| w.wake_by_ref());
        }
      },
      wifi_event_t::WIFI_EVENT_STA_CONNECTED => {
        let event = unsafe { &*(event_data as *const wifi_event_sta_connected_t) };

        eprintln!("EVENT_DATA: {:?}", event);

        let ssid = Ssid(event.ssid);
        let bssid = MacAddr6::from(event.bssid);
        let channel = NonZeroU8::new(event.channel);
        let auth_mode = AuthMode::from(event.authmode);

        f.state = ConnectFutureState::ConnectedWithoutIp { ssid, bssid, channel, auth_mode };

        eprintln!("EVENT_STATE: {:?}", f.state);
      },
      wifi_event_t::WIFI_EVENT_STA_DISCONNECTED => {
        let event = unsafe { &*(event_data as *const wifi_event_sta_disconnected_t) };

        eprintln!("EVENT_DATA: {:?}", event);

        let ssid = Ssid(event.ssid);
        let bssid = MacAddr6::from(event.bssid);
        let reason: wifi_err_reason_t = unsafe { transmute(event.reason as u32) };

        let error = ConnectionError {
          ssid, bssid, reason
        };

        f.state = ConnectFutureState::Failed(WifiError::ConnectionError(error));

        eprintln!("EVENT_STATE: {:?}", f.state);

        f.waker.as_ref().map(|w| w.wake_by_ref());
      },
      _ => (),
    }
  } else if event_base == unsafe { IP_EVENT } {
    let event_id: ip_event_t = unsafe { transmute(event_id) };

    eprintln!("IP_EVENT: {:?}", event_id);

    match event_id {
      ip_event_t::IP_EVENT_STA_GOT_IP => {
        let event = unsafe { &*(event_data as *const ip_event_got_ip_t) };

        let ip_info = unsafe { IpInfo::from_native_unchecked(event.ip_info) };

        eprintln!("EVENT_DATA: {:?}", event);

        if let ConnectFutureState::ConnectedWithoutIp { ssid, bssid, channel, auth_mode } = mem::replace(&mut f.state, ConnectFutureState::Starting) {
          f.state = ConnectFutureState::Connected { ip_info, ssid, bssid, channel, auth_mode };
        } else {
          unreachable!();
        }

        eprintln!("EVENT_STATE: {:?}", f.state);

        f.waker.as_ref().map(|w| w.wake_by_ref());
      },
      _ => (),
    }
  }
}

