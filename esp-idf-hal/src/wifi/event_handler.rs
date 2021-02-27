use esp_idf_bindgen::{esp_event_base_t, esp_event_handler_register, esp_event_handler_unregister};

use crate::EspError;

#[derive(Debug)]
pub struct EventHandler {
  base: esp_event_base_t,
  id: i32,
  handler: extern "C" fn(*mut libc::c_void, *const i8, i32, *mut libc::c_void),
}

impl EventHandler {
  pub fn register(
    base: esp_event_base_t,
    id: i32,
    handler: extern "C" fn(*mut libc::c_void, *const i8, i32, *mut libc::c_void),
    arg: *mut libc::c_void
  ) -> Result<Self, EspError> {
    esp_ok!(esp_event_handler_register(base, id, Some(handler), arg))?;
    Ok(Self { base, id, handler })
  }
}

impl Drop for EventHandler {
  fn drop(&mut self) {
    let _ = esp_ok!(esp_event_handler_unregister(self.base, self.id, Some(self.handler)));
  }
}
