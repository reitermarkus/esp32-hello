use core::mem::{self, transmute, MaybeUninit};
use std::str::Utf8Error;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering::SeqCst};
use core::task::{Poll, Context, Waker};
use core::pin::Pin;

use core::fmt;
use macaddr::MacAddr6;

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

/// An instance of the WiFi peripheral.
#[must_use = "WiFi will be stopped and deinitialized immediately. Drop it explicitly after you are done using it or create a named binding."]
#[derive(Debug)]
pub struct Wifi<T = ()> {
  ap_mode: Option<ApMode>,
  sta_mode: Option<StaMode>,
  config: T,
  deinit_on_drop: bool,
  ip_info: Option<IpInfo>,
}

#[cfg(target_device = "esp8266")]
fn initialize_network_interface() {
  unsafe { tcpip_adapter_init() };
}

#[cfg(target_device = "esp32")]
fn initialize_network_interface() {
  static NETIF_STATE: AtomicU8 = AtomicU8::new(0);

  loop {
    match NETIF_STATE.compare_and_swap(0, 1, SeqCst) {
      0 => {
        esp_ok!(esp_netif_init()).expect("failed to initialize network interface");
        NETIF_STATE.store(2, SeqCst);
        return;
      },
      1 => continue,
      _ => return,
    }
  }
}

fn event_loop_create_default() {
  static EVENT_LOOP_STATE: AtomicU8 = AtomicU8::new(0);

  loop {
    match EVENT_LOOP_STATE.compare_and_swap(0, 1, SeqCst) {
      0 => {
        esp_ok!(esp_event_loop_create_default()).expect("failed to initialize default event loop");
        EVENT_LOOP_STATE.store(2, SeqCst);
        return;
      },
      1 => continue,
      _ => return,
    }
  }
}

static AP_COUNT: AtomicU8 = AtomicU8::new(0);
static STA_COUNT: AtomicU8 = AtomicU8::new(0);

fn get_mode() -> Result<wifi_mode_t, EspError> {
  let mut mode = wifi_mode_t::WIFI_MODE_NULL;
  esp_ok!(esp_wifi_get_mode(&mut mode))?;
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
  if AP_COUNT.fetch_add(1, SeqCst) > 0 {
    return Ok(())
  }

  let current_mode = get_mode()?;

  let new_mode = match current_mode {
    wifi_mode_t::WIFI_MODE_AP | wifi_mode_t::WIFI_MODE_APSTA => return Ok(()),
    wifi_mode_t::WIFI_MODE_NULL => wifi_mode_t::WIFI_MODE_AP,
    wifi_mode_t::WIFI_MODE_STA => wifi_mode_t::WIFI_MODE_APSTA,
    _ => unreachable!(),
  };

  esp_ok!(esp_wifi_set_mode(new_mode))
}

fn leave_ap_mode() -> Result<(), EspError> {
  if AP_COUNT.fetch_sub(1, SeqCst) != 1 {
    return Ok(())
  }

  let current_mode = get_mode()?;

  match current_mode {
    wifi_mode_t::WIFI_MODE_AP => {
      esp_ok!(esp_wifi_stop())
    },
    wifi_mode_t::WIFI_MODE_APSTA => {
      esp_ok!(esp_wifi_set_mode(wifi_mode_t::WIFI_MODE_STA))
    },
    _ => unreachable!(),
  }
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
  if STA_COUNT.fetch_add(1, SeqCst) > 0 {
    return Ok(())
  }

  let current_mode = get_mode()?;

  let new_mode = match current_mode {
    wifi_mode_t::WIFI_MODE_STA | wifi_mode_t::WIFI_MODE_APSTA => return Ok(()),
    wifi_mode_t::WIFI_MODE_NULL => wifi_mode_t::WIFI_MODE_STA,
    wifi_mode_t::WIFI_MODE_AP => wifi_mode_t::WIFI_MODE_APSTA,
    _ => unreachable!(),
  };

  esp_ok!(esp_wifi_set_mode(new_mode))
}

fn leave_sta_mode() -> Result<(), EspError> {
  if STA_COUNT.fetch_sub(1, SeqCst) != 1 {
    return Ok(())
  }

  let current_mode = get_mode()?;

  match current_mode {
    wifi_mode_t::WIFI_MODE_STA => {
      esp_ok!(esp_wifi_stop())
    },
    wifi_mode_t::WIFI_MODE_APSTA => {
      esp_ok!(esp_wifi_set_mode(wifi_mode_t::WIFI_MODE_AP))
    },
    _ => unreachable!(),
  }
}

static WIFI_ACTIVE: AtomicBool = AtomicBool::new(false);

impl Wifi {
  /// Take the WiFi peripheral if it is not already in use.
  pub fn take() -> Option<Wifi> {
    if WIFI_ACTIVE.compare_and_swap(false, true, SeqCst) {
      None
    } else {
      initialize_network_interface();

      event_loop_create_default();

      NonVolatileStorage::init_default().expect("failed to initialize default NVS partition");
      let config = wifi_init_config_t::default();
      esp_ok!(esp_wifi_init(&config)).expect("failed to initialize WiFi with default configuration");

      Some(Wifi { ap_mode: None, sta_mode: None, config: (), deinit_on_drop: true, ip_info: None })
    }
  }

  /// Start an access point using the specified [`ApConfig`](struct.ApConfig.html).
  pub fn start_ap(mut self, config: ApConfig) -> Result<WifiRunning, WifiError> {
    self.deinit_on_drop = false;

    let interface = Interface::Ap;
    interface.init();
    let mut ap_config = wifi_config_t::from(&config);

    let ap_mode = ApMode::enter();

    if let Err(err) = esp_ok!(esp_wifi_set_config(wifi_interface_t::WIFI_IF_AP, &mut ap_config)).and_then(|_| {
      esp_ok!(esp_wifi_start())
    }) {
      return Err(err.into());
    }
    Ok(WifiRunning::Ap(Wifi { ap_mode: Some(ap_mode), sta_mode: self.sta_mode.take(), config, deinit_on_drop: true, ip_info: Some(interface.ip_info()) }))
  }

  /// Connect to a WiFi network using the specified [`StaConfig`](struct.StaConfig.html).
  pub fn connect_sta(mut self, config: StaConfig) -> ConnectFuture {
    self.deinit_on_drop = false;

    Interface::Sta.init();

    let sta_mode = Some(StaMode::enter());

    let mut sta_config = wifi_config_t::from(&config);
    let state = if let Err(err) = esp_ok!(esp_wifi_set_config(wifi_interface_t::WIFI_IF_STA, &mut sta_config)) {
        ConnectFutureState::Failed(err.into())
    } else {
      ConnectFutureState::Starting
    };

    ConnectFuture { waker: None, wifi: Some(self), config: Some(config), sta_mode, state, handlers: None }
  }
}

/// A running WiFi instance.
#[must_use = "WiFi will be stopped and deinitialized immediately. Drop it explicitly after you are done using it or create a named binding."]
#[derive(Debug)]
pub enum WifiRunning {
  Sta(Wifi<StaConfig>),
  Ap(Wifi<ApConfig>),
}

impl WifiRunning {
  pub fn scan(&mut self, scan_config: &ScanConfig) -> ScanFuture {
    match self {
      Self::Sta(wifi) => wifi.scan(scan_config),
      Self::Ap(wifi) => wifi.scan(scan_config),
    }
  }

  pub fn ip_info(&self) -> &IpInfo {
    match self {
      Self::Sta(wifi) => wifi.ip_info(),
      Self::Ap(wifi) => wifi.ip_info(),
    }
  }
}

impl<T> Wifi<T> {
  /// Scan nearby WiFi networks using the specified [`ScanConfig`](struct.ScanConfig.html).
  pub fn scan(&mut self, scan_config: &ScanConfig) -> ScanFuture {
    ScanFuture::new(scan_config)
  }

  pub fn config(&self) -> &T {
    &self.config
  }
}

impl<T> Drop for Wifi<T> {
  /// Stops a running WiFi instance and deinitializes it, making it available again
  /// by calling [`Wifi::take()`](struct.Wifi.html#method.take).
  fn drop(&mut self) {
    if self.deinit_on_drop {
      if mem::size_of::<T>() != 0 {
        unsafe { esp_wifi_stop() };
      }

      let _ = esp_ok!(esp_wifi_deinit());
      NonVolatileStorage::deinit_default();

      WIFI_ACTIVE.store(false, SeqCst);
    }
  }
}

impl Wifi<StaConfig> {
  pub fn ip_info(&self) -> &IpInfo {
    self.ip_info.as_ref().unwrap()
  }

  /// Stop a running WiFi in station mode.
  pub fn stop(mut self) -> (StaConfig, Wifi) {
    self.deinit_on_drop = false;
    let config = MaybeUninit::uninit();
    let config = mem::replace(&mut self.config, unsafe { config.assume_init() });
    (config, Wifi { ap_mode: self.ap_mode.take(), sta_mode: None, config: (), deinit_on_drop: true, ip_info: None })
  }
}

impl Wifi<ApConfig> {
  pub fn ip_info(&self) -> &IpInfo {
    self.ip_info.as_ref().unwrap()
  }

  /// Stop a running WiFi access point.
  pub fn stop(mut self) -> (ApConfig, Wifi) {
    self.deinit_on_drop = false;
    let config = MaybeUninit::uninit();
    let config = mem::replace(&mut self.config, unsafe { config.assume_init() });
    (config, Wifi { ap_mode: None, sta_mode: self.sta_mode.take(), config: (), deinit_on_drop: true, ip_info: None })
  }
}

#[derive(Debug)]
enum ConnectFutureState {
  Failed(WifiError),
  Starting,
  ConnectedWithoutIp { ssid: Ssid, bssid: MacAddr6, channel: u8, auth_mode: AuthMode },
  Connected { ip_info: Option<IpInfo>, ssid: Ssid, bssid: MacAddr6, channel: u8, auth_mode: AuthMode },
}

/// A future representing an ongoing connection to an access point.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct ConnectFuture {
  waker: Option<Waker>,
  wifi: Option<Wifi>,
  config: Option<StaConfig>,
  sta_mode: Option<StaMode>,
  state: ConnectFutureState,
  handlers: Option<[EventHandler; 4]>,
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
pub enum WifiError<T = ()> {
  /// An internal error not directly related to WiFi.
  Internal(T, EspError),
  /// A connection error returned when a [`ConnectFuture`](struct.ConnectFuture.html) fails.
  ConnectionError(T, ConnectionError),
}

impl WifiError<()> {
  pub(crate) fn with_wifi<W>(self, wifi: W) -> WifiError<W> {
    match self {
      Self::Internal(_, esp_error) => WifiError::Internal(wifi, esp_error),
      Self::ConnectionError(_, error) => WifiError::ConnectionError(wifi, error),
    }
  }
}

impl<T> WifiError<Wifi<T>> {
  /// Create a new uninitialized [`Wifi`](struct.Wifi.html) instance.
  pub fn wifi(self) -> Wifi<T> {
    match self {
      Self::Internal(wifi, _) => wifi,
      Self::ConnectionError(wifi, _) => wifi,
    }
  }
}

impl From<EspError> for WifiError<()> {
  fn from(esp_error: EspError) -> Self {
    Self::Internal((), esp_error)
  }
}

impl<T> fmt::Display for WifiError<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Internal(_, esp_error) => esp_error.fmt(f),
      Self::ConnectionError(_, error) => error.fmt(f),
    }
  }
}

impl core::future::Future for ConnectFuture {
  type Output = Result<WifiRunning, WifiError<Wifi>>;

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
            return Poll::Ready(Err(WifiError::from(err).with_wifi(self.wifi.take().unwrap())));
          },
        }

        if let Err(err) = esp_ok!(esp_wifi_start()) {
          return Poll::Ready(Err(WifiError::from(err).with_wifi(self.wifi.take().unwrap())));
        }

        Poll::Pending
      },
      ConnectFutureState::Failed(ref err) => {
        return Poll::Ready(Err(WifiError::from(err.clone()).with_wifi(self.wifi.take().unwrap())));
      },
      ConnectFutureState::ConnectedWithoutIp { .. } => {
        Poll::Pending
      },
      ConnectFutureState::Connected { ref mut ip_info, .. } => {
        let ip_info = ip_info.take();
        let config = self.config.take().unwrap();
        let wifi = self.wifi.as_mut().unwrap();
        Poll::Ready(Ok(WifiRunning::Sta(Wifi { ap_mode: wifi.ap_mode.take(), sta_mode: self.sta_mode.take(), config, deinit_on_drop: true, ip_info })))
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

        let ssid = Ssid { ssid: event.ssid, ssid_len: event.ssid_len as usize };
        let bssid = MacAddr6::from(event.bssid);
        let channel = event.channel;
        let auth_mode = AuthMode::from(event.authmode);

        f.state = ConnectFutureState::ConnectedWithoutIp { ssid, bssid, channel, auth_mode };

        eprintln!("EVENT_STATE: {:?}", f.state);
      },
      wifi_event_t::WIFI_EVENT_STA_DISCONNECTED => {
        let event = unsafe { &*(event_data as *const wifi_event_sta_disconnected_t) };

        eprintln!("EVENT_DATA: {:?}", event);

        let ssid = Ssid { ssid: event.ssid, ssid_len: event.ssid_len as usize };
        let bssid = MacAddr6::from(event.bssid);
        let reason: wifi_err_reason_t = unsafe { transmute(event.reason as u32) };

        let error = ConnectionError {
          ssid, bssid, reason
        };

        f.state = ConnectFutureState::Failed(WifiError::ConnectionError((), error));

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
          f.state = ConnectFutureState::Connected { ip_info: Some(ip_info), ssid, bssid, channel, auth_mode };
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

