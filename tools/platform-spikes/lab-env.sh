# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Environment of the WP-04 platform lab (DL-138). Source this file; it has no side effects.
# Everything the lab writes goes to LAB_HOME (outside the repository, not on the system volume).
# Overrides: LAB_HOME, MAIN_SDK, LAB_JAVA_HOME, LAB_PORT.

: "${LAB_HOME:=/Volumes/BackCat/rivqen-p2-lab}"
# The main Android SDK is only read (a copy of cmdline-tools and the accepted licenses); never written.
: "${MAIN_SDK:=$HOME/Library/Android/sdk}"
# JDK 21 for every lab build, also when the shell has another JAVA_HOME (override: LAB_JAVA_HOME).
JAVA_HOME="${LAB_JAVA_HOME:-$HOME/.gradle/jdks/azul_systems__inc_-21-aarch64-os_x.2/zulu21.50.19-ca-jdk21.0.11-macosx_aarch64/Contents/Home}"

LAB_SDK="$LAB_HOME/sdk"
export LAB_HOME MAIN_SDK JAVA_HOME LAB_SDK
export ANDROID_SDK_ROOT="$LAB_SDK"
export ANDROID_HOME="$LAB_SDK"
export ANDROID_AVD_HOME="$LAB_HOME/avd"
export ANDROID_USER_HOME="$LAB_HOME/user"
export ANDROID_EMULATOR_HOME="$LAB_HOME/user"
export GRADLE_USER_HOME="$LAB_HOME/gradle"
export CARGO_TARGET_DIR="$LAB_HOME/cargo-target"
export TMPDIR="$LAB_HOME/tmp/"
# Xcode build output of the iOS prototype and the FFI benchmark.
LAB_DERIVED="$LAB_HOME/derived"
LAB_TLS="$LAB_HOME/tls"
LAB_OUT="$LAB_HOME/out"
export LAB_DERIVED LAB_TLS LAB_OUT

LAB_ADB="$LAB_SDK/platform-tools/adb"
LAB_EMULATOR="$LAB_SDK/emulator/emulator"
LAB_SDKMANAGER="$LAB_SDK/cmdline-tools/latest/bin/sdkmanager"
LAB_AVDMANAGER="$LAB_SDK/cmdline-tools/latest/bin/avdmanager"
export LAB_ADB LAB_EMULATOR LAB_SDKMANAGER LAB_AVDMANAGER

# Android images and AVDs of H-128. One emulator at a time, always on this console port, so every
# adb call names the lab serial and never reaches emulator-5554 or another device.
LAB_IMAGE_37="system-images;android-37.0;google_apis;arm64-v8a"
LAB_IMAGE_29="system-images;android-29;google_apis;arm64-v8a"
LAB_AVD_37=rivqen-p2-api37
LAB_AVD_29=rivqen-p2-api29
: "${LAB_PORT:=5602}"
LAB_SERIAL="emulator-$LAB_PORT"
export LAB_IMAGE_37 LAB_IMAGE_29 LAB_AVD_37 LAB_AVD_29 LAB_PORT LAB_SERIAL

# iOS simulators of H-128. The lab creates its own devices with this name prefix and deletes them
# at the end; it never boots, changes or deletes another simulator.
LAB_SIM_PREFIX=rivqen-p2
# iOS 27.1 (24A94401) supports only the device type iPhone-Duo, so iOS 27 runs on 27.0 (24A434).
# iPhone 16 Pro is the newest iPhone that all three runtimes support (one model for every version).
LAB_SIM_DEVICE_TYPE="com.apple.CoreSimulator.SimDeviceType.iPhone-16-Pro"
LAB_SIM_OS="27.0 26.5 18.5"
export LAB_SIM_PREFIX LAB_SIM_DEVICE_TYPE LAB_SIM_OS

# Spike host ports (CONTRACT.md section 1).
LAB_HOST_PORT=18443
LAB_HOST_BAD_PORT=18444
LAB_HOST_CONTROL_PORT=18445
LAB_SITE_PORT=18480
export LAB_HOST_PORT LAB_HOST_BAD_PORT LAB_HOST_CONTROL_PORT LAB_SITE_PORT
