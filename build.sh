#!/usr/bin/env bash

# Workaround until https://github.com/espressif/esp-idf/pull/6587 is merged.
export ZSH_VERSION=''

set -euo pipefail

cd "$(dirname "${0}")"

CHIP=esp32
PACKAGE=app
EXAMPLE=
FLASH_BAUDRATE=460800
MONITOR_BAUDRATE=115200
PROFILE=
ERASE_FLASH=false

while (( ${#@} )); do
  case "${1}" in
    --chip)
      shift
      CHIP="${1}"
      ;;
    -p|--package)
      shift
      PACKAGE="${1}"
      ;;
    --example)
      shift
      EXAMPLE="${1}"
      ;;
    --release)
      PROFILE=release
      ;;
    --flash-baudrate)
      shift
      FLASH_BAUDRATE="${1}"
      ;;
    --erase-flash)
      ERASE_FLASH=true
      ;;
    *)
      echo "Invalid argument: '${1}'" >&2
      exit 1
      ;;
    esac
  shift
done

SERIAL_PORT="$(find /dev -name 'tty.usbserial-*' 2>/dev/null | head -n 1 || true)"

TARGET="xtensa-${CHIP}-none-elf"

if [[ "${CHIP}" == esp32 ]]; then
  IDF_PATH="$(pwd)/esp-idf"
else
  IDF_PATH="$(pwd)/ESP8266_RTOS_SDK"
fi

export IDF_PATH

IDF_TOOLS_PATH="$(pwd)/esp-idf-tools"
export IDF_TOOLS_PATH

mkdir -p "${IDF_TOOLS_PATH}"

idf_version="$(git -C "${IDF_PATH}" rev-parse HEAD 2>/dev/null || true)"
version_file="${IDF_TOOLS_PATH}/installed_version"

if ! [[ -f "${version_file}" ]] || [[ "${idf_version}" != "$(cat "${version_file}")" ]]; then
  case "${CHIP}" in
    esp32)
      "${IDF_PATH}/install.sh"
      ;;
    esp8266)
      python -m pip install --user -r "${IDF_PATH}/requirements.txt"
      ;;
  esac

  if [[ -n "${idf_version:-}" ]]; then
    echo "${idf_version}" > "${version_file}"
  fi
fi

export PATH="$(brew --prefix rust-xtensa)/bin:$(brew --prefix llvm-xtensa)/bin:${PATH}"

source "${IDF_PATH}/export.sh" >/dev/null


if [[ "${CHIP}" = 'esp32' ]]; then
  ln -sfn sdkconfig.esp32 app/sdkconfig
else
  ln -sfn sdkconfig.esp8266 app/sdkconfig
fi

export CARGO_TARGET_DIR="$(pwd)/target"

cargo build ${PROFILE:+"--${PROFILE}"} --target "${TARGET}" ${PACKAGE:+--package "${PACKAGE}"} ${EXAMPLE:+--example "${EXAMPLE}"}

cargo doc ${PROFILE:+"--${PROFILE}"} --target "${TARGET}" --no-deps

if [[ -z "${SERIAL_PORT}" ]]; then
  exit
fi

esptool() {
  "${IDF_PATH}/components/esptool_py/esptool/esptool.py" --chip "${CHIP}" --port "${SERIAL_PORT}" ${FLASH_BAUDRATE:+--baud "${FLASH_BAUDRATE}"} "${@}" | \
    grep -E -v 'esptool.py|Serial port|Changing baud rate|Changed.|Uploading stub|Running stub|Stub running|Configuring flash size|Leaving'
}

mapfile -t binary_targets < <(
  cargo metadata --format-version 1 \
  | jq -c '
      .workspace_members as $members | .packages
      | map(select(.id as $id | $members[] | contains($id) ))
      | map(.targets)[] | map(select(.kind[] | contains("bin") or contains("example")))[]
      | .name
    ' -r
)

for binary_target in "${binary_targets[@]}"; do
  for t in "${CARGO_TARGET_DIR}"/${TARGET}/{release,debug}/{,examples/}"${binary_target}"; do
    if [[ -f "${t}" ]]; then
      "${IDF_PATH}/components/esptool_py/esptool/esptool.py" \
        --chip "${CHIP}" \
        elf2image \
        --version "${ELF2IMAGE_VERSION}" \
        -o "${t}.bin" \
        "${t}" | tail -n +2
    fi
  done
done

FLASH_ARGS=( -z --flash_mode dio --flash_freq 80m --flash_size detect )

if [[ "${CHIP}" = 'esp32' ]]; then
  BOOTLOADER_OFFSET=0x1000
else
  BOOTLOADER_OFFSET=0x0000
fi
BOOTLOADER_BINARY="target/${TARGET}/esp-build/bootloader/bootloader.bin"
PARTITION_TABLE_OFFSET=0x8000
PARTITION_TABLE_BINARY="target/${TARGET}/esp-build/partitions.bin"
APPLICATION_OFFSET=0x10000
if [[ -n "${EXAMPLE-}" ]]; then
  BINARY_PATH="examples/${EXAMPLE}"
else
  BINARY_PATH="${PACKAGE}"
fi
APPLICATION_BINARY="target/${TARGET}/${PROFILE:-debug}/${BINARY_PATH}.bin"

if "${ERASE_FLASH}"; then
  echo 'Erasing flash …'
  esptool --after no_reset erase_flash
fi

echo "Verifying bootloader …"
if esptool --no-stub --after no_reset verify_flash "${BOOTLOADER_OFFSET}" "${BOOTLOADER_BINARY}" &>/dev/null; then
  echo 'Bootloader is up to date.'
else
  echo 'Flashing new bootloader …'
  esptool --after no_reset write_flash "${FLASH_ARGS[@]}" \
    "${BOOTLOADER_OFFSET}" "${BOOTLOADER_BINARY}"
fi

echo "Verifying partition table …"
if esptool --no-stub --after no_reset verify_flash "${PARTITION_TABLE_OFFSET}" "${PARTITION_TABLE_BINARY}" &>/dev/null; then
  echo 'Partition table is up to date.'
else
  echo 'Flashing new partition table …'
  esptool --after no_reset write_flash "${FLASH_ARGS[@]}" \
    "${PARTITION_TABLE_OFFSET}" "${PARTITION_TABLE_BINARY}"
fi

echo "Verifying application …"
if esptool --no-stub --after no_reset verify_flash "${APPLICATION_OFFSET}" "${APPLICATION_BINARY}" &>/dev/null; then
  echo 'Application table is up to date.'
else
  echo 'Flashing new application …'
  esptool --after no_reset write_flash "${FLASH_ARGS[@]}" \
    "${APPLICATION_OFFSET}" "${APPLICATION_BINARY}"
fi

python -m serial.tools.miniterm --raw --exit-char=3 --rts=0 --dtr=0 "${SERIAL_PORT}" "${MONITOR_BAUDRATE}"
