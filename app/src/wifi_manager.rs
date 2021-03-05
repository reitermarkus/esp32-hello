use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::str;
use std::time::Duration;

use esp_idf_hal::{nvs::NameSpace, wifi::*};

/// Try parsing `Ssid` and `Password` from URL parameters.
fn ssid_and_password(params: &[u8]) -> (Option<Ssid>, Option<Password>) {
  let mut ssid = None;
  let mut password = None;

  for (name, value) in url::form_urlencoded::parse(&params) {
    match name.as_ref() {
      "ssid" => ssid = Ssid::from_bytes(value.as_bytes()).ok(),
      "password" => password = Password::from_bytes(value.as_bytes()).ok(),
      _ => if ssid.is_some() && password.is_some() { break },
    }
  }

  (ssid, password)
}

fn write_ok(client: &mut TcpStream) -> io::Result<()> {
  writeln!(client, "HTTP/1.1 200 OK")?;
  writeln!(client, "Content-Type: text/html")?;
  writeln!(client)
}

fn write_template(client: &mut TcpStream) -> io::Result<()> {
  write_ok(client)?;
  writeln!(client, "{}", include_str!("index.html"))
}

async fn handle_index(wifi: Arc<Mutex<Option<Wifi>>>, mut client: TcpStream) -> io::Result<()> {
  write_template(&mut client)?;

  writeln!(client, r##"
    <script type='text/javascript'>
      function showPassword(checkbox) {{
        const input = document.getElementById('password')
        input.type = checkbox.checked ? 'text' : 'password'
      }}
    </script>
    <form action='/connect' method='post'>
      <input name='ssid' type='text' maxlength='32' required='true' placeholder='SSID' list='ssids'/>
      <input id='password' name='password' type='password' maxlength='64' placeholder='Password'>
      <input id='show-password' type='checkbox' onclick='showPassword(this)'> <label for='show-password'>Show Password</label>
      <input type='submit' value='Connect'>
    </form>
  "##)?;

  let scan_config = ScanConfig::builder()
    .show_hidden(true)
    .scan_type(ScanType::Passive { max: Duration::from_millis(100) })
    .build();

  writeln!(client, "<datalist id='ssids'>")?;

  if let Some(wifi) = &mut *wifi.lock().unwrap() {
    match wifi.scan(&scan_config).await {
      Ok(mut aps) => {
        aps.sort_by(|a, b| a.ssid().cmp(b.ssid()));
        aps.dedup_by(|a, b| a.ssid() == b.ssid());

        for ssid in aps.iter().map(|ap| ap.ssid()).filter(|ssid| !ssid.is_empty()) {
          writeln!(client, "<option>{}</option>", ssid)?;
        }

      },
      Err(err) => {
        eprintln!("WiFi scan failed: {}", err);
      }
    }
  }

  writeln!(client, "</datalist>")?;

  Ok(())
}

fn handle_hotspot_detect(mut client: TcpStream) -> io::Result<()> {
  writeln!(client, "HTTP/1.1 303 See Other")?;
  writeln!(client, "Location: /")?;
  writeln!(client, "Content-Type: text/plain")?;
  writeln!(client)?;
  writeln!(client, "Redirecting …")
}

fn handle_connection_error(mut client: TcpStream, message: &str) -> io::Result<()> {
  write_template(&mut client)?;
  writeln!(client, "<p class='error'>Failed to connect.{} <a href='./'>Retry?</a></p>", message)
}

fn handle_connection_success(mut client: TcpStream, message: &str) -> io::Result<()> {
  write_template(&mut client)?;
  writeln!(client, "<p class='success'>Success.{}</p>", message)
}

fn handle_not_found(mut client: TcpStream) -> io::Result<()> {
  writeln!(client, "HTTP/1.1 404 Not Found")?;
  writeln!(client)
}

fn handle_internal_error(mut client: TcpStream) -> io::Result<()> {
  writeln!(client, "HTTP/1.1 500 INTERNAL SERVER ERROR")?;
  writeln!(client)
}

pub async fn handle_request(
  mut client: TcpStream, addr: SocketAddr,
  wifi_storage: Arc<Mutex<NameSpace>>,
  wifi_running: Arc<Mutex<Option<Wifi>>>,
) {
  println!("Handling request from {} …", addr);

  let mut buf: [u8; 1024] = [0; 1024];
  let len = match client.read(&mut buf) {
    Ok(len) => len,
    Err(err) => {
      eprintln!("Error reading from client: {:?}", err);
      let _ = handle_internal_error(client);
      return;
    },
  };

  let mut headers = [httparse::EMPTY_HEADER; 16];
  let mut req = httparse::Request::new(&mut headers);

  let status = req.parse(&buf);

  let res = match (status, req.method, req.path) {
    (Ok(httparse::Status::Complete(header_len)), Some(method), Some(path)) => {
      println!("{} {} - {} bytes", method, path, len);

      match (method, path) {
        ("GET", "/") => handle_index(Arc::clone(&wifi_running), client).await,
        ("GET", "/hotspot-detect.html") => handle_hotspot_detect(client),
        ("POST", "/connect") => {
          let body = &buf[header_len..len];

          if let (Some(ssid), Some(password)) = ssid_and_password(body) {
            let mut wifi_storage = wifi_storage.lock().unwrap();

            wifi_storage.set::<&str>("ssid", &ssid.as_str()).expect("Failed saving SSID");
            wifi_storage.set::<&str>("password", &password.as_str()).expect("Failed saving password");

            let mut wifi_running = wifi_running.lock().unwrap();
            let wifi = wifi_running.take().unwrap();
            let ap_config = wifi.as_ap().unwrap().config();

            let message = format!(" Connecting to “{}” …", ssid.as_str());
            let res = handle_connection_success(client, &message);

            let wifi = wifi.stop_ap();
            match connect_ssid_password(wifi, ssid, password).await {
              Ok(wifi) => {
                wifi_running.replace(wifi);
              },
              Err(err) => {
                wifi_running.replace(err.wifi().start_ap(ap_config).expect("Failed to start access point"));
              }
            }

            res
          } else {
            handle_connection_error(client, " SSID is empty.")
          }
        },
        _ => handle_not_found(client),
      }
    }
    _ => handle_internal_error(client),
  };

  if let Err(err) = res {
    eprintln!("Error handling request: {}", err);
  }
}

/// Try to connect to an access point with the given `ssid` and `password` in station mode, otherwise revert to access point mode.
pub async fn connect_ssid_password(wifi: Wifi, ssid: Ssid, password: Password) -> Result<Wifi, WifiError<Wifi>> {
  let sta_config = StaConfig::builder()
    .ssid(ssid)
    .password(password)
    .build();

  eprintln!("Connecting to '{}' with password '{}' …", sta_config.ssid(), sta_config.password());

  match wifi.connect_sta(sta_config).await {
    Ok(wifi) => {
      if let Some(sta) = wifi.as_sta() {
        eprintln!("Connected to '{}' with IP '{}'.", sta.config().ssid(), sta.ip_info().ip());
      }
      Ok(wifi)
    },
    Err(err) => Err(err),
  }
}
