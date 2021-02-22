use core::mem::transmute;
use std::str::{self, FromStr, Utf8Error};
use std::ops::Deref;
use std::cmp::{Eq, Ord, Ordering};
use core::ptr;
use std::mem::{self, MaybeUninit};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering::SeqCst};
use core::task::{Poll, Context, Waker};
use core::pin::Pin;
use alloc::boxed::Box;

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

const SSID_MAX_LEN: usize = 32;
const PASSWORD_MAX_LEN: usize = 64;

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

/// A WiFi SSID.
#[derive(Clone)]
pub struct Ssid {
  ssid: [u8; SSID_MAX_LEN],
  ssid_len: usize,
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

/// A WiFi password.
#[derive(Clone)]
pub struct Password {
  password: [u8; PASSWORD_MAX_LEN],
  password_len: usize,
}

impl Password {
  pub fn as_str(&self) -> &str {
    &unsafe { str::from_utf8_unchecked(&self.password[..self.password_len]) }
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

  pub unsafe fn from_bytes_unchecked(bytes: &[u8]) -> Password {
    let password_len = bytes.len();
    let mut password = [0; PASSWORD_MAX_LEN];
    ptr::copy_nonoverlapping(bytes.as_ptr(), password.as_mut_ptr(), password_len);
    Self { password, password_len }
  }
}

impl Default for Password {
  fn default() -> Self {
    Self { password: [0; PASSWORD_MAX_LEN], password_len: 0 }
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
      .field("password", &"********")
      .field("password_len", &8)
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

/// An instance of the WiFi peripheral.
#[must_use = "WiFi will be stopped and deinitialized immediately. Drop it explicitly after you are done using it or create a named binding."]
#[derive(Debug)]
pub struct Wifi<T = ()> {
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

fn enter_ap_mode() {
  if AP_COUNT.fetch_add(1, SeqCst) > 0 {
    return
  }

  let current_mode = get_mode().expect("failed to get WiFi mode");

  let new_mode = match current_mode {
    wifi_mode_t::WIFI_MODE_AP | wifi_mode_t::WIFI_MODE_APSTA => return,
    wifi_mode_t::WIFI_MODE_NULL => wifi_mode_t::WIFI_MODE_AP,
    wifi_mode_t::WIFI_MODE_STA => wifi_mode_t::WIFI_MODE_APSTA,
    _ => unreachable!(),
  };

  esp_ok!(esp_wifi_set_mode(new_mode)).expect("failed to set WiFi mode");
}

fn leave_ap_mode() {
  if AP_COUNT.fetch_sub(1, SeqCst) != 1 {
    return
  }

  let current_mode = get_mode().expect("failed to get WiFi mode");

  match current_mode {
    wifi_mode_t::WIFI_MODE_AP => {
      esp_ok!(esp_wifi_stop()).expect("failed to stop WiFi");
    },
    wifi_mode_t::WIFI_MODE_APSTA => {
      esp_ok!(esp_wifi_set_mode(wifi_mode_t::WIFI_MODE_STA)).expect("failed to set WiFi mode");
    },
    _ => unreachable!(),
  };
}

fn enter_sta_mode() {
  if STA_COUNT.fetch_add(1, SeqCst) > 0 {
    return
  }

  let current_mode = get_mode().expect("failed to get WiFi mode");

  let new_mode = match current_mode {
    wifi_mode_t::WIFI_MODE_STA | wifi_mode_t::WIFI_MODE_APSTA => return,
    wifi_mode_t::WIFI_MODE_NULL => wifi_mode_t::WIFI_MODE_STA,
    wifi_mode_t::WIFI_MODE_AP => wifi_mode_t::WIFI_MODE_APSTA,
    _ => unreachable!(),
  };

  esp_ok!(esp_wifi_set_mode(new_mode)).expect("failed to set WiFi mode");
}

fn leave_sta_mode() {
  if STA_COUNT.fetch_sub(1, SeqCst) != 1 {
    return
  }

  let current_mode = get_mode().expect("failed to get WiFi mode");

  match current_mode {
    wifi_mode_t::WIFI_MODE_STA => {
      esp_ok!(esp_wifi_stop()).expect("failed to stop WiFi");
    },
    wifi_mode_t::WIFI_MODE_APSTA => {
      esp_ok!(esp_wifi_set_mode(wifi_mode_t::WIFI_MODE_AP)).expect("failed to set WiFi mode");
    },
    _ => unreachable!(),
  };
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

      Some(Wifi { config: (), deinit_on_drop: true, ip_info: None })
    }
  }

  /// Start an access point using the specified [`ApConfig`](struct.ApConfig.html).
  pub fn start_ap(mut self, config: ApConfig) -> Result<WifiRunning, WifiError> {
    self.deinit_on_drop = false;

    let interface = Interface::Ap;
    interface.init();
    let mut ap_config = wifi_config_t::from(&config);
    enter_ap_mode();
    esp_ok!(esp_wifi_set_config(wifi_interface_t::WIFI_IF_AP, &mut ap_config))?;
    esp_ok!(esp_wifi_start())?;
    Ok(WifiRunning::Ap(Wifi { config, deinit_on_drop: true, ip_info: Some(interface.ip_info()) }))
  }

  /// Connect to a WiFi network using the specified [`StaConfig`](struct.StaConfig.html).
  pub fn connect_sta(mut self, config: StaConfig) -> ConnectFuture {
    self.deinit_on_drop = false;

    Interface::Sta.init();
    let mut sta_config = wifi_config_t::from(&config);

    enter_sta_mode();

    let state = if let Err(err) = esp_ok!(esp_wifi_set_config(wifi_interface_t::WIFI_IF_STA, &mut sta_config)) {
      ConnectFutureState::Failed(err.into())
    } else {
      ConnectFutureState::Starting
    };

    ConnectFuture { config, state }
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
    leave_sta_mode();
    let config = MaybeUninit::uninit();
    let config = mem::replace(&mut self.config, unsafe { config.assume_init() });
    (config, Wifi { config: (), deinit_on_drop: true, ip_info: None })
  }
}

impl Wifi<ApConfig> {
  pub fn ip_info(&self) -> &IpInfo {
    self.ip_info.as_ref().unwrap()
  }

  /// Stop a running WiFi access point.
  pub fn stop(mut self) -> (ApConfig, Wifi) {
    self.deinit_on_drop = false;
    leave_ap_mode();
    let config = MaybeUninit::uninit();
    let config = mem::replace(&mut self.config, unsafe { config.assume_init() });
    (config, Wifi { config: (), deinit_on_drop: true, ip_info: None })
  }
}

#[derive(Debug)]
enum ConnectFutureState {
  Failed(WifiError),
  Starting,
  ConnectedWithoutIp { ssid: Ssid, bssid: MacAddr6, channel: u8, auth_mode: AuthMode },
  Connected { ip_info: IpInfo, ssid: Ssid, bssid: MacAddr6, channel: u8, auth_mode: AuthMode },
}

/// A future representing an ongoing connection to an access point.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct ConnectFuture {
  config: StaConfig,
  state: ConnectFutureState,
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

impl WifiError {
  /// Create a new uninitialized [`Wifi`](struct.Wifi.html) instance.
  pub fn wifi(self) -> Wifi {
    Wifi { config: (), deinit_on_drop: true, ip_info: None }
  }
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

impl core::future::Future for ConnectFuture {
  type Output = Result<WifiRunning, WifiError>;

  #[cfg(target_device = "esp8266")]
  fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
    Poll::Pending
  }

  #[cfg(target_device = "esp32")]
  fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
    fn register_sta_handlers(b: *mut (Pin<&mut ConnectFuture>, &Waker)) -> Result<(), EspError> {
      esp_ok!(esp_event_handler_register(
        WIFI_EVENT, wifi_event_t::WIFI_EVENT_STA_START as _, Some(wifi_sta_handler), b as *mut _,
      ))?;
      esp_ok!(esp_event_handler_register(
        WIFI_EVENT, wifi_event_t::WIFI_EVENT_STA_CONNECTED as _, Some(wifi_sta_handler), b as *mut _,
      ))?;
      esp_ok!(esp_event_handler_register(
        WIFI_EVENT, wifi_event_t::WIFI_EVENT_STA_DISCONNECTED as _, Some(wifi_sta_handler), b as *mut _,
      ))?;
      esp_ok!(esp_event_handler_register(
        IP_EVENT, ip_event_t::IP_EVENT_STA_GOT_IP as _, Some(wifi_sta_handler), b as *mut _,
      ))
    }

    fn unregister_sta_handlers() -> Result<(), EspError> {
      esp_ok!(esp_event_handler_unregister(
        WIFI_EVENT, wifi_event_t::WIFI_EVENT_STA_START as _, Some(wifi_sta_handler),
      )).and(esp_ok!(esp_event_handler_unregister(
        WIFI_EVENT, wifi_event_t::WIFI_EVENT_STA_CONNECTED as _, Some(wifi_sta_handler),
      ))).and(esp_ok!(esp_event_handler_unregister(
        WIFI_EVENT, wifi_event_t::WIFI_EVENT_STA_DISCONNECTED as _, Some(wifi_sta_handler),
      ))).and(esp_ok!(esp_event_handler_unregister(
        IP_EVENT, ip_event_t::IP_EVENT_STA_GOT_IP as _, Some(wifi_sta_handler),
      )))
    }

    match &self.state {
      ConnectFutureState::Starting => {
        let b: Box<(Pin<&mut ConnectFuture>, &Waker)> = Box::new((self.as_mut(), cx.waker()));
        let b = Box::into_raw(b);

        if let Err(err) = register_sta_handlers(b) {
          let _ = unregister_sta_handlers();
          drop(unsafe { Box::from_raw(b) });
          return Poll::Ready(Err(err.into()));
        }

        esp_ok!(esp_wifi_start())?;
        Poll::Pending
      },
      ConnectFutureState::ConnectedWithoutIp { .. } => {
        Poll::Pending
      }
      _ => {
        if let Err(err) = unregister_sta_handlers() {
          if !matches!(self.state, ConnectFutureState::Failed(..)) {
            self.state = ConnectFutureState::Failed(err.into())
          }
        }

        match self.state {
          ConnectFutureState::Starting | ConnectFutureState::ConnectedWithoutIp { .. } => unreachable!(),
          ConnectFutureState::Failed(ref err) => {
            leave_sta_mode();
            Poll::Ready(Err(err.clone().into()))
          },
          ConnectFutureState::Connected { ref mut ip_info, .. } => {
            let ip_info = mem::replace(ip_info, unsafe { MaybeUninit::uninit().assume_init() });
            let config = mem::replace(&mut self.as_mut().config, unsafe { MaybeUninit::uninit().assume_init() });
            Poll::Ready(Ok(WifiRunning::Sta(Wifi { config, deinit_on_drop: true, ip_info: Some(ip_info) })))
          }
        }
      }
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
  if event_base == unsafe { WIFI_EVENT } {
    let event_id: wifi_event_t = unsafe { transmute(event_id) };

    eprintln!("WIFI_EVENT: {:?}", event_id);

    match event_id {
      wifi_event_t::WIFI_EVENT_STA_START => {
        if let Err(err) = esp_ok!(esp_wifi_connect()) {
          let (mut f, waker) = unsafe { *Box::from_raw(event_handler_arg as *mut (Pin<&mut ConnectFuture>, &Waker)) };
          f.state = ConnectFutureState::Failed(err.into());
          waker.wake_by_ref();
        }
      },
      wifi_event_t::WIFI_EVENT_STA_CONNECTED => {
        let event = unsafe { &*(event_data as *const wifi_event_sta_connected_t) };

        eprintln!("EVENT_DATA: {:?}", event);

        let ssid = Ssid { ssid: event.ssid, ssid_len: event.ssid_len as usize };
        let bssid = MacAddr6::from(event.bssid);
        let channel = event.channel;
        let auth_mode = AuthMode::from(event.authmode);

        let (ref mut f, _) = unsafe { &mut *(event_handler_arg as *mut (Pin<&mut ConnectFuture>, &Waker)) };
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

        let (mut f, waker) = unsafe { *Box::from_raw(event_handler_arg as *mut (Pin<&mut ConnectFuture>, &Waker)) };
        f.state = ConnectFutureState::Failed(WifiError::ConnectionError(error));

        eprintln!("EVENT_STATE: {:?}", f.state);

        waker.wake_by_ref();
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

        let (mut f, waker) = unsafe { *Box::from_raw(event_handler_arg as *mut (Pin<&mut ConnectFuture>, &Waker)) };

        if let ConnectFutureState::ConnectedWithoutIp { ssid, bssid, channel, auth_mode } = mem::replace(&mut f.state, ConnectFutureState::Starting) {
          f.state = ConnectFutureState::Connected { ip_info, ssid, bssid, channel, auth_mode };
        } else {
          unreachable!();
        }

        eprintln!("EVENT_STATE: {:?}", f.state);

        waker.wake_by_ref();
      },
      _ => (),
    }
  }
}
