#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Create the Android lab in LAB_HOME (DL-102): a lab SDK root with the emulator, platform-tools and
# the API 28 image, the AVD and GRADLE_USER_HOME. It skips what exists, so you can run it again.
# Usage: sh tools/upstream-lab/android/env.sh
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
. "$HERE/lab-env.sh"

case "$LAB_HOME" in
  /Volumes/*/*) ;;
  *) echo "env.sh: LAB_HOME must be a directory on an external volume (/Volumes/<name>/...), got $LAB_HOME" >&2; exit 2 ;;
esac
[ -d "$(dirname "$LAB_HOME")" ] || { echo "env.sh: the volume of $LAB_HOME is not mounted" >&2; exit 2; }
[ -x "$MAIN_SDK/cmdline-tools/latest/bin/sdkmanager" ] || { echo "env.sh: no cmdline-tools in $MAIN_SDK" >&2; exit 2; }
[ -x "$JAVA_HOME/bin/java" ] || { echo "env.sh: JAVA_HOME has no java: $JAVA_HOME" >&2; exit 2; }

echo "== disk before"; df -h / "$(dirname "$LAB_HOME")"
mkdir -p "$LAB_SDK" "$ANDROID_AVD_HOME" "$ANDROID_USER_HOME" "$GRADLE_USER_HOME" "$TMPDIR" "$LAB_HOME/logs"

# cmdline-tools: a copy of the main SDK tools, so that sdkmanager and avdmanager use the lab SDK root.
if [ ! -x "$LAB_SDKMANAGER" ]; then
  mkdir -p "$LAB_SDK/cmdline-tools"
  cp -R "$MAIN_SDK/cmdline-tools/latest" "$LAB_SDK/cmdline-tools/latest"
fi
# Licenses: the license hashes that the owner of the main SDK accepted (no new acceptance).
if [ ! -d "$LAB_SDK/licenses" ]; then
  cp -R "$MAIN_SDK/licenses" "$LAB_SDK/licenses"
fi

need=""
[ -x "$LAB_EMULATOR" ] || need="$need emulator"
[ -x "$LAB_ADB" ] || need="$need platform-tools"
img_dir="$LAB_SDK/$(echo "$LAB_IMAGE" | tr ';' '/')"
[ -f "$img_dir/system.img" ] || need="$need $LAB_IMAGE"
if [ -n "$need" ]; then
  echo "== sdkmanager install:$need"
  # shellcheck disable=SC2086
  "$LAB_SDKMANAGER" --sdk_root="$LAB_SDK" --install $need
fi
[ -f "$img_dir/system.img" ] || { echo "env.sh: the image is not installed: $LAB_IMAGE" >&2; exit 1; }

if [ ! -f "$ANDROID_AVD_HOME/$LAB_AVD.ini" ]; then
  echo "== create AVD $LAB_AVD"
  echo no | "$LAB_AVDMANAGER" create avd --force -n "$LAB_AVD" -k "$LAB_IMAGE" -d pixel
  cfg="$ANDROID_AVD_HOME/$LAB_AVD.avd/config.ini"
  # Small fixed AVD: 2 GiB RAM, 2 GiB data partition, no camera, no audio, software GPU, cold boot.
  for kv in "hw.ramSize=2048" "disk.dataPartition.size=2G" "hw.camera.back=none" "hw.camera.front=none" \
            "hw.audioInput=no" "hw.audioOutput=no" "hw.gpu.enabled=yes" "hw.gpu.mode=swiftshader_indirect" \
            "hw.keyboard=yes" "fastboot.forceColdBoot=yes" "hw.sdCard=no"; do
    key="${kv%%=*}"
    grep -v "^$key[ =]" "$cfg" > "$cfg.tmp" || true
    echo "$kv" >> "$cfg.tmp"
    mv "$cfg.tmp" "$cfg"
  done
fi

echo "== lab ready: $LAB_HOME"
du -sh "$LAB_SDK" "$ANDROID_AVD_HOME" "$GRADLE_USER_HOME"
echo "== disk after"; df -h / "$(dirname "$LAB_HOME")"
