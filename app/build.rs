use std::{env, error::Error, fs::{create_dir_all, File}, io::stderr, os::unix::{io::{FromRawFd, AsRawFd}}, path::PathBuf, process::Command};

use jobserver::Client;

fn main() -> Result<(), Box<dyn Error>> {
  println!("cargo:rerun-if-changed=Makefile");
  println!("cargo:rerun-if-changed=components/compiler_builtins/atomics.c");
  println!("cargo:rerun-if-changed=components/compiler_builtins/component.mk");
  println!("cargo:rerun-if-changed=main/app_main.c");
  println!("cargo:rerun-if-changed=main/component.mk");
  println!("cargo:rerun-if-changed=partitions.csv");
  println!("cargo:rerun-if-changed=sdkconfig");

  let target = env::var("TARGET").expect("TARGET is unset");
  let target_dir = PathBuf::from(env::var("CARGO_TARGET_DIR").expect("CARGO_TARGET_DIR is unset"));

  let idf_path = PathBuf::from(env::var("IDF_PATH").expect("IDF_PATH is unset"));

  env::set_var("CC", "xtensa-esp32-elf-cc");
  env::set_var("CXX", "xtensa-esp32-elf-c++");

  let esp_build_path = target_dir.join(target).join("esp-build");
  create_dir_all(&esp_build_path)?;

  let client = unsafe { Client::from_env().expect("failed to connect to jobserver") };

  let stderr = unsafe { File::from_raw_fd(stderr().as_raw_fd()) };

  let cargo_makeflags = env::var_os("CARGO_MAKEFLAGS").expect("CARGO_MAKEFLAGS is unset");

  let mut cmd = Command::new("make");
  cmd.arg("bootloader");
  cmd.env("MAKEFLAGS", &cargo_makeflags);
  cmd.env("VERBOSE", "1");
  cmd.stdout(stderr.try_clone()?);
  cmd.stderr(stderr.try_clone()?);

  client.configure(&mut cmd);

  let status = cmd.status()?;
  assert!(status.success());

  let mut cmd = Command::new("make");
  cmd.arg("app");
  cmd.env("MAKEFLAGS", &cargo_makeflags);
  cmd.env("VERBOSE", "1");
  cmd.stdout(stderr.try_clone()?);
  cmd.stderr(stderr.try_clone()?);

  client.configure(&mut cmd);

  let status = cmd.status()?;
  assert!(status.success());

  let link_args = vec![
    "-nostdlib",

    "-ucall_user_start_cpu0",

    "-Wl,--gc-sections",
    "-Wl,-static",
    "-Wl,--start-group",

    "-L${ESP_BUILD}/esp32",
    "-L${IDF_PATH}/components/esp32/ld",           "-Tesp32_out.ld",
                                                   "-uld_include_panic_highint_hdl",
                                                   "-T${ESP_BUILD}/esp32/esp32.project.ld",
                                                   "-Tesp32.peripherals.ld",
    "-L${ESP_BUILD}/app_update",                   "-lapp_update",
                                                   "-uesp_app_desc",
    "-L${ESP_BUILD}/bootloader_support",           "-lbootloader_support",
    "-L${ESP_BUILD}/compiler_builtins",            "-lcompiler_builtins",
    "-L${ESP_BUILD}/driver",                       "-ldriver",
    "-L${ESP_BUILD}/efuse",                        "-lefuse",
    "-L${ESP_BUILD}/esp_common",                   "-lesp_common",
    "-L${ESP_BUILD}/esp_eth",                      "-lesp_eth",
    "-L${ESP_BUILD}/esp_event",                    "-lesp_event",
    "-L${ESP_BUILD}/esp_hw_support",               "-lesp_hw_support",
    "-L${ESP_BUILD}/esp_ipc",                      "-lesp_ipc",
    "-L${ESP_BUILD}/esp_netif",                    "-lesp_netif",
    "-L${ESP_BUILD}/esp_pm",                       "-lesp_pm",
    "-L${ESP_BUILD}/esp_ringbuf",                  "-lesp_ringbuf",
    "-L${ESP_BUILD}/esp_rom",                      "-lesp_rom",
    "-L${IDF_PATH}/components/esp_rom/esp32/ld",   "-Tesp32.rom.ld",
                                                   "-Tesp32.rom.api.ld",
                                                   "-Tesp32.rom.libgcc.ld",
                                                   "-Tesp32.rom.syscalls.ld",
                                                   "-Tesp32.rom.newlib-data.ld",
                                                   "-Tesp32.rom.newlib-funcs.ld",
                                                   "-Tesp32.rom.newlib-time.ld",
    "-L${ESP_BUILD}/esp_system",                   "-lesp_system",
    "-L${ESP_BUILD}/esp_timer",                    "-lesp_timer",
    "-L${ESP_BUILD}/esp_wifi",                     "-lesp_wifi",
    "-L${IDF_PATH}/components/esp_wifi/lib/esp32", "-lcore",
                                                   "-lrtc",
                                                   "-lnet80211",
                                                   "-lpp",
                                                   "-lsmartconfig",
                                                   "-lcoexist",
                                                   "-lespnow",
                                                   "-lphy",
                                                   "-lmesh",
    "-L${ESP_BUILD}/esp-tls",                      "-lesp-tls",
    "-L${ESP_BUILD}/esp32",                        "-lesp32",
    "-L${ESP_BUILD}/freertos",                     "-lfreertos",
                                                   "-Wl,--undefined=uxTopUsedPriority",
    "-L${ESP_BUILD}/hal",                          "-lhal",
    "-L${ESP_BUILD}/heap",                         "-lheap",
    "-L${ESP_BUILD}/log",                          "-llog",
    "-L${ESP_BUILD}/mbedtls",                      "-lmbedtls",
    "-L${ESP_BUILD}/mdns",                         "-lmdns",
    "-L${ESP_BUILD}/newlib",                       "-lnewlib",
                                                   "-lc",
                                                   "-lm",
                                                   "-unewlib_include_locks_impl",
                                                   "-unewlib_include_heap_impl",
                                                   "-unewlib_include_syscalls_impl",
    "-L${ESP_BUILD}/nvs_flash",                    "-lnvs_flash",
    "-L${ESP_BUILD}/lwip",                         "-llwip",
    "-L${ESP_BUILD}/pthread",                      "-lpthread",
                                                   "-upthread_include_pthread_impl",
                                                   "-upthread_include_pthread_cond_impl",
                                                   "-upthread_include_pthread_local_storage_impl",
    "-L${ESP_BUILD}/soc",                          "-lsoc",
    "-L${ESP_BUILD}/spi_flash",                    "-lspi_flash",
    "-L${ESP_BUILD}/tcpip_adapter",                "-ltcpip_adapter",
    "-L${ESP_BUILD}/vfs",                          "-lvfs",
    "-L${ESP_BUILD}/wpa_supplicant",               "-lwpa_supplicant",
    "-L${ESP_BUILD}/xtensa",                       "-lxtensa",
    "-L${ESP_BUILD}/xtensa",                       "-lxtensa",
    "-L${IDF_PATH}/components/xtensa/esp32",       "-lxt_hal",

    "-lgcc",
    "-lstdc++",
    "-lgcov",
    "-Wl,--end-group",
    "-Wl,-EL",
    "-fno-rtti",
  ];

  for arg in link_args {
    let arg = arg.replace("${ESP_BUILD}", &esp_build_path.display().to_string())
                 .replace("${IDF_PATH}", &idf_path.display().to_string());
    println!("cargo:rustc-link-arg={}", arg);
  }

  Ok(())
}
