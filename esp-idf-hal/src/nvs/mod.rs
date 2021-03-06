use core::ptr;
use core::mem::MaybeUninit;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::ffi::CString;

use esp_idf_bindgen::{
  esp_err_t,
  ESP_ERR_NVS_NO_FREE_PAGES,
  ESP_ERR_NVS_NEW_VERSION_FOUND,
  nvs_open_mode_t,
  nvs_handle_t,
  nvs_flash_init_partition,
  nvs_flash_erase_partition,
  nvs_flash_deinit_partition,
  nvs_open_from_partition,
  nvs_close,
  NVS_DEFAULT_PART_NAME,
  ESP_ERR_NVS_INVALID_NAME,
};

use super::*;

mod get_set;
pub use get_set::*;

/// A non-volatile storage partition.
#[derive(Debug)]
pub struct NonVolatileStorage {
  partition_name: CString,
}

/// A namespace on a non-volatile storage partition.
#[derive(Debug)]
pub struct NameSpace {
  handle: nvs_handle_t,
}

impl NameSpace {
  pub fn get<T: NvsGet>(&self, key: &str) -> Result<T, EspError> {
    let key = CString::new(key).map_err(|_| EspError { code: ESP_ERR_NVS_INVALID_NAME as esp_err_t })?;
    T::nvs_get(self, key.as_ref())
  }

  pub fn set<T: NvsSet>(&mut self, key: &str, value: T) -> Result<(), EspError> {
    let key = CString::new(key).map_err(|_| EspError { code: ESP_ERR_NVS_INVALID_NAME as esp_err_t })?;
    value.nvs_set(self, key.as_ref())
  }
}

impl Drop for NameSpace {
  fn drop(&mut self) {
    unsafe { nvs_close(self.handle) };
  }
}

const DEFAULT_PART_NAME: &'static CStr = unsafe { CStr::from_bytes_with_nul_unchecked(NVS_DEFAULT_PART_NAME) };
static DEFAULT_INSTANCES: AtomicUsize = AtomicUsize::new(0);

impl NonVolatileStorage {
  /// Open a non-volatile storage partition.
  pub fn open(name: &str) -> Result<NonVolatileStorage, EspError> {
    let partition_name = CString::new(name).map_err(|_| EspError { code: ESP_ERR_NVS_INVALID_NAME as esp_err_t })?;
    Self::open_cstring(partition_name)
  }

  fn open_cstring(partition_name: CString) -> Result<NonVolatileStorage, EspError> {
    if partition_name.as_c_str() == DEFAULT_PART_NAME {
      Self::init_default()?;
    } else {
      Self::init(&partition_name)?;
    }

    Ok(Self { partition_name })
  }

  /// Open a namespace on a non-volatile storage partition.
  pub fn namespace(&mut self, name: &str) -> Result<NameSpace, EspError> {
    let name = CString::new(name).map_err(|_| EspError { code: ESP_ERR_NVS_INVALID_NAME as esp_err_t })?;

    let mut handle = MaybeUninit::<nvs_handle_t>::uninit();

    esp_ok!(nvs_open_from_partition(
      self.partition_name.as_ptr(),
      name.as_ptr(),
      nvs_open_mode_t::NVS_READWRITE,
      handle.as_mut_ptr(),
    ))?;

    Ok(NameSpace { handle: unsafe { handle.assume_init() } })
  }

  fn init(partition_name: &CStr) -> Result<(), EspError> {
    esp_ok!(nvs_flash_init_partition(partition_name.as_ptr()))
  }

  fn erase(partition_name: &CStr) -> Result<(), EspError> {
    esp_ok!(nvs_flash_erase_partition(partition_name.as_ptr()))
  }

  pub(crate) fn init_default() -> Result<(), EspError> {
    loop {
      match DEFAULT_INSTANCES.compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) => {
          let res = match Self::init(DEFAULT_PART_NAME) {
            Err(err) if err.code == ESP_ERR_NVS_NO_FREE_PAGES as esp_err_t || err.code == ESP_ERR_NVS_NEW_VERSION_FOUND as esp_err_t => {
              let _ = Self::erase(DEFAULT_PART_NAME);
              Self::init(DEFAULT_PART_NAME)
            },
            res => res,
          };

          return match res {
            Ok(()) => {
              DEFAULT_INSTANCES.fetch_add(1, Ordering::SeqCst);
              Ok(())
            },
            Err(err) => {
              DEFAULT_INSTANCES.store(0, Ordering::SeqCst);
              Err(err)
            }
          }
        },
        Err(1) => continue,
        _ => {
          DEFAULT_INSTANCES.fetch_add(1, Ordering::SeqCst);
          return Ok(())
        },
      }
    }
  }

  pub(crate) fn deinit_default() {
    loop {
      match DEFAULT_INSTANCES.compare_exchange(2, 1, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) => {
          unsafe { nvs_flash_deinit_partition(DEFAULT_PART_NAME.as_ptr()) };
          DEFAULT_INSTANCES.fetch_sub(1, Ordering::SeqCst);
          return;
        },
        Err(1) => continue,
        _ => {
          DEFAULT_INSTANCES.fetch_sub(1, Ordering::SeqCst);
          return
        },
      }
    }
  }
}

impl Default for NonVolatileStorage {
  fn default() -> Self {
    Self::open_cstring(DEFAULT_PART_NAME.to_owned()).expect("failed to initialize default NVS partition")
  }
}

impl Drop for NonVolatileStorage {
  fn drop(&mut self) {
    if self.partition_name.as_c_str() == DEFAULT_PART_NAME {
      Self::deinit_default();
    } else {
      unsafe { nvs_flash_deinit_partition(self.partition_name.as_ptr()) };
    }
  }
}
