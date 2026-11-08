#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Create the platform lab in LAB_HOME (DL-138): the lab SDK with the emulator, platform-tools and
# the two images of H-128, the AVDs rivqen-p2-api37 and rivqen-p2-api29, and the work folders.
# It skips what exists, so a second run changes nothing. It never writes the main SDK.
# Usage: sh tools/platform-spikes/lab/env.sh [--dry-run]
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
. "$HERE/../lab-env.sh"
. "$HERE/common.sh"
require_lab_root
[ -x "$MAIN_SDK/cmdline-tools/latest/bin/sdkmanager" ] || { echo "env.sh: no cmdline-tools in $MAIN_SDK" >&2; exit 2; }
[ -x "$JAVA_HOME/bin/java" ] || { echo "env.sh: JAVA_HOME has no java: $JAVA_HOME" >&2; exit 2; }

echo "== disk before"; df -h / "$(dirname "$LAB_HOME")"
run mkdir -p "$LAB_SDK" "$ANDROID_AVD_HOME" "$ANDROID_USER_HOME" "$GRADLE_USER_HOME" "$LAB_HOME/tmp" \
  "$LAB_HOME/logs" "$CARGO_TARGET_DIR" "$LAB_DERIVED" "$LAB_TLS" "$LAB_OUT"

# cmdline-tools and licenses: copies from the main SDK (read only; no new license acceptance).
if [ ! -x "$LAB_SDKMANAGER" ]; then
  run mkdir -p "$LAB_SDK/cmdline-tools"
  run cp -R "$MAIN_SDK/cmdline-tools/latest" "$LAB_SDK/cmdline-tools/latest"
fi
if [ ! -d "$LAB_SDK/licenses" ]; then
  run cp -R "$MAIN_SDK/licenses" "$LAB_SDK/licenses"
fi

image_dir() { echo "$LAB_SDK/$(echo "$1" | tr ';' '/')"; }

need=""
[ -x "$LAB_EMULATOR" ] || need="$need emulator"
[ -x "$LAB_ADB" ] || need="$need platform-tools"
for img in "$LAB_IMAGE_37" "$LAB_IMAGE_29"; do
  [ -f "$(image_dir "$img")/system.img" ] || need="$need $img"
done
if [ -n "$need" ]; then
  free=$(system_free_gb)
  [ "$free" -ge 20 ] || { echo "env.sh: only $free GB free on the system volume (need 20)" >&2; exit 1; }
  echo "== sdkmanager install:$need"
  # shellcheck disable=SC2086
  run "$LAB_SDKMANAGER" --sdk_root="$LAB_SDK" --install $need
fi

make_avd() { # name image
  [ -f "$ANDROID_AVD_HOME/$1.ini" ] && return 0
  echo "== create AVD $1"
  if [ "$LAB_DRY_RUN" = 1 ]; then echo "dry-run: avdmanager create avd -n $1 -k $2"; return 0; fi
  [ -f "$(image_dir "$2")/system.img" ] || { echo "env.sh: the image is not installed: $2" >&2; exit 1; }
  echo no | "$LAB_AVDMANAGER" create avd --force -n "$1" -k "$2" -d pixel
  cfg="$ANDROID_AVD_HOME/$1.avd/config.ini"
  # Small fixed AVD: 2 GiB RAM, 4 GiB data, no camera or audio, software GPU, cold boot.
  for kv in "hw.ramSize=2048" "disk.dataPartition.size=4G" "hw.camera.back=none" "hw.camera.front=none" \
            "hw.audioInput=no" "hw.audioOutput=no" "hw.gpu.enabled=yes" "hw.gpu.mode=swiftshader_indirect" \
            "hw.keyboard=yes" "fastboot.forceColdBoot=yes" "hw.sdCard=no"; do
    key="${kv%%=*}"
    grep -v "^$key[ =]" "$cfg" > "$cfg.tmp" || true
    echo "$kv" >> "$cfg.tmp"
    mv "$cfg.tmp" "$cfg"
  done
}
make_avd "$LAB_AVD_37" "$LAB_IMAGE_37"
make_avd "$LAB_AVD_29" "$LAB_IMAGE_29"

echo "== lab ready: $LAB_HOME"
du -sh "$LAB_SDK" "$ANDROID_AVD_HOME" 2>/dev/null || true
echo "== disk after"; df -h / "$(dirname "$LAB_HOME")"
