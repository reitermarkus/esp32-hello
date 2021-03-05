#![feature(never_type)]
#![cfg_attr(not(doc), no_main)]

use std::sync::{Arc, Mutex};
use std::thread::{self, sleep};
use std::time::Duration;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::net::TcpListener;

use embedded_hal::digital::v2::OutputPin;
use macaddr::MacAddr;

use esp_idf_hal::{*, interface::*, nvs::*, wifi::*};

use futures::executor::block_on;

mod wifi_manager;
use wifi_manager::*;

mod dns;

#[no_mangle]
pub fn app_main() {
  block_on(async {
    if let Err(err) = rust_blink_and_write().await {
      println!("{}", err);
    }
  })
}

async fn rust_blink_and_write() -> Result<!, EspError> {
    use esp32_hal::{target, gpio::GpioExt};

    let dp = unsafe { target::Peripherals::steal() };
    let pins = dp.GPIO.split();

    let mut gpio = pins.gpio22.into_open_drain_output();

    let mut nvs = NonVolatileStorage::default();

    let wifi = Wifi::take().unwrap();

    println!("AP started.");

    let namespace = nvs.namespace("wifi")?;
    println!("namespace: {:?}", namespace);

    let t = thread::Builder::new()
      .name("hello_thread".into())
      .stack_size(2048)
      .spawn(|| {
        println!("HELLO, WORLD!");
        42
      });

    println!("Thread spawn result: {:?}", t);
    println!("Thread join result: {:?}", t.map(|t| t.join().unwrap()));

    thread::Builder::new()
      .name("dns_thread".into())
      .stack_size(8192)
      .spawn(dns::server)
      .unwrap();

    thread::Builder::new()
      .name("blink_thread".into())
      .stack_size(1024)
      .spawn(move || {
        loop {
          gpio.set_low().unwrap();
          sleep(Duration::from_millis(100));
          gpio.set_high().unwrap();
          sleep(Duration::from_secs(3));
        }
      })
      .unwrap();

    thread::Builder::new()
      .name("server_thread".into())
      .stack_size(8192)
      .spawn(move || block_on(async {
        let mac = MacAddr::from(Interface::Ap);

        let ap_ssid = Ssid::from_bytes(format!("ESP {}", mac).as_bytes()).unwrap();

        let ap_config = ApConfig::builder()
          .ssid(ap_ssid)
          .build();

        let wifi_storage = namespace;

        let ssid = wifi_storage.get::<String>("ssid").ok().and_then(|s| Ssid::from_bytes(s.as_bytes()).ok());
        let password = wifi_storage.get::<String>("password").ok().and_then(|s| Password::from_bytes(s.as_bytes()).ok());

        let wifi_running;

        if let (Some(ssid), Some(password)) = (ssid, password) {
          wifi_running = match wifi_manager::connect_ssid_password(wifi, ssid, password).await {
            Ok(wifi) => wifi,
            Err(err) => {
              println!("Starting Access Point '{}' …", ap_config.ssid());
              err.wifi().start_ap(ap_config).expect("Failed to start access point")
            },
          };
        } else {
          println!("Starting Access Point '{}' …", ap_config.ssid());
          wifi_running = wifi.start_ap(ap_config).expect("Failed to start access point");
        }

        let stream = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 80)).expect("failed starting TCP listener");

        let wifi_running = Arc::new(Mutex::new(Some(wifi_running)));
        let wifi_storage = Arc::new(Mutex::new(wifi_storage));

        loop {
          thread::yield_now();

          let client = stream.accept().and_then(|(client, addr)| {
            client.set_read_timeout(Some(Duration::from_secs(30)))?;
            client.set_write_timeout(Some(Duration::from_secs(30)))?;
            Ok((client, addr))
          });

          match client {
            Ok((client, addr)) => {
              let wifi_storage = Arc::clone(&wifi_storage);
              let wifi_running = Arc::clone(&wifi_running);

              thread::Builder::new()
                .stack_size(8192)
                .spawn(move || block_on(async {
                  handle_request(client, addr, wifi_storage, wifi_running).await
                }))
                .unwrap();
            },
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(e) => eprintln!("Client error: {}", e),
          }
        }
      }))
      .unwrap();

    loop {
      sleep(Duration::from_secs(5))
    }
  }
