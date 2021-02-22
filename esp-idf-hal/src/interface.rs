use std::mem::MaybeUninit;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::ptr;

use esp_idf_bindgen::{esp_mac_type_t, esp_read_mac};
#[cfg(target_device = "esp8266")]
use esp_idf_bindgen::{tcpip_adapter_get_ip_info, tcpip_adapter_if_t, tcpip_adapter_ip_info_t as ip_info_t};
#[cfg(target_device = "esp32")]
use esp_idf_bindgen::{esp_netif_get_ip_info, esp_netif_ip_info_t as ip_info_t, esp_netif_t, esp_netif_create_default_wifi_ap, esp_netif_create_default_wifi_sta};
use macaddr::{MacAddr, MacAddr6};

static AP_PTR: AtomicUsize = AtomicUsize::new(0);
static STA_PTR: AtomicUsize = AtomicUsize::new(0);
const INIT_SENTINEL: usize = usize::max_value();

/// Enumeration of all available interfaces.
#[derive(Debug, Clone, Copy)]
pub enum Interface {
  /// WiFi interface in station mode.
  Sta,
  /// WiFi interface in access point mode.
  Ap,
  /// Bluetooth interface.
  #[cfg(target_device = "esp32")]
  Bt,
  /// Ethernet interface.
  #[cfg(target_device = "esp32")]
  Eth,
}

impl Interface {
  #[cfg(target_device = "esp8266")]
  pub fn ip_info(&self) -> IpInfo {
    let interface = match self {
      Self::Ap => tcpip_adapter_if_t::TCPIP_ADAPTER_IF_AP,
      Self::Sta => tcpip_adapter_if_t::TCPIP_ADAPTER_IF_STA,
    };

    let mut ip_info = MaybeUninit::<ip_info_t>::uninit();
    esp_ok!(tcpip_adapter_get_ip_info(interface, ip_info.as_mut_ptr())).unwrap(); // Can only fail with invalid arguments.
    unsafe { IpInfo::from_native_unchecked(ip_info.assume_init()) }
  }


  #[cfg(target_device = "esp8266")]
  pub(crate) fn init(&self) {
  }

  #[cfg(target_device = "esp32")]
  pub fn ip_info(&self) -> IpInfo {
    let mut ip_info = MaybeUninit::<ip_info_t>::uninit();
    esp_ok!(esp_netif_get_ip_info(self.ptr(), ip_info.as_mut_ptr())).unwrap(); // Can only fail if `self.ptr()` returns `NUL`, in which case the interface does not support IPs.
    unsafe { IpInfo::from_native_unchecked(ip_info.assume_init()) }
  }

  #[cfg(target_device = "esp32")]
  fn ptr(&self) -> *mut esp_netif_t {
    match self {
      Self::Ap => {
        loop {
          match AP_PTR.compare_and_swap(0, INIT_SENTINEL, Ordering::SeqCst) {
            0 => {
              let ptr = unsafe { esp_netif_create_default_wifi_ap() };
              AP_PTR.store(ptr as _, Ordering::SeqCst);
              return ptr;
            },
            INIT_SENTINEL => continue,
            ptr => return ptr as _,
          }
        }
      },
      Self::Sta => {
        loop {
          match STA_PTR.compare_and_swap(0, INIT_SENTINEL, Ordering::SeqCst) {
            0 => {
              let ptr = unsafe { esp_netif_create_default_wifi_sta() };
              STA_PTR.store(ptr as _, Ordering::SeqCst);
              return ptr;
            },
            INIT_SENTINEL => continue,
            ptr => return ptr as _,
          }
        }
      },
      _ => ptr::null_mut()
    }
  }

  #[cfg(target_device = "esp32")]
  pub(crate) fn init(&self) {
    self.ptr();
  }
}

/// ```no_run
/// use macaddr::MacAddr6;
/// use esp32_hal::Interface;
///
/// MacAddr6::from(Interface::Ap)
/// ```
impl From<Interface> for MacAddr6 {
  fn from(interface: Interface) -> Self {
    let mac_address_type = match interface {
      Interface::Sta => esp_mac_type_t::ESP_MAC_WIFI_STA,
      Interface::Ap  => esp_mac_type_t::ESP_MAC_WIFI_SOFTAP,
      #[cfg(target_device = "esp32")]
      Interface::Bt  => esp_mac_type_t::ESP_MAC_BT,
      #[cfg(target_device = "esp32")]
      Interface::Eth => esp_mac_type_t::ESP_MAC_ETH,
    };

    let mut mac_address = MaybeUninit::<Self>::uninit();
    esp_ok!(esp_read_mac(mac_address.as_mut_ptr() as *mut _, mac_address_type)).unwrap(); // Can only fail with invalid arguments.
    unsafe { mac_address.assume_init() }
  }
}

/// ```no_run
/// use macaddr::MacAddr;
/// use esp32_hal::Interface;
///
/// MacAddr::from(Interface::Ap)
/// ```
impl From<Interface> for MacAddr {
  fn from(interface: Interface) -> Self {
    Self::V6(interface.into())
  }
}

/// IP information for an [`Interface`](enum.Interface.html).
#[derive(Debug, Clone)]
pub struct IpInfo {
  ip: Ipv4Addr,
  netmask: Ipv4Addr,
  gateway: Ipv4Addr,
}

impl IpInfo {
  pub fn ip(&self) -> &Ipv4Addr {
    &self.ip
  }

  pub fn netmask(&self) -> &Ipv4Addr {
    &self.netmask
  }

  pub fn gateway(&self) -> &Ipv4Addr {
    &self.gateway
  }
}

impl IpInfo {
  pub(crate) unsafe fn from_native_unchecked(ip_info: ip_info_t) -> Self {
    IpInfo {
      ip: u32::from_be(ip_info.ip.addr).into(),
      netmask: u32::from_be(ip_info.netmask.addr).into(),
      gateway: u32::from_be(ip_info.gw.addr).into(),
    }
  }
}
