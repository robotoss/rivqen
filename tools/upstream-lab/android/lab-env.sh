# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Environment of the Android client lab (DL-102). Source this file; it has no side effects.
# Everything the lab writes goes to LAB_HOME (outside the repository, not on the system volume).
# Overrides: LAB_HOME, MAIN_SDK, JAVA_HOME, LAB_IMAGE, LAB_AVD.

: "${LAB_HOME:=/Volumes/BackCat/rivqen-lab}"
# The main Android SDK is read and used (platforms, build-tools, a copy of cmdline-tools); never written.
: "${MAIN_SDK:=$HOME/Library/Android/sdk}"
: "${JAVA_HOME:=$HOME/.gradle/jdks/azul_systems__inc_-21-aarch64-os_x.2/zulu21.50.19-ca-jdk21.0.11-macosx_aarch64/Contents/Home}"
# Emulator image (DL-102): API 28 default arm64-v8a; fallback: system-images;android-29;default;arm64-v8a.
: "${LAB_IMAGE:=system-images;android-28;default;arm64-v8a}"
: "${LAB_AVD:=rivqen-api28}"

LAB_SDK="$LAB_HOME/sdk"
export LAB_HOME MAIN_SDK JAVA_HOME LAB_IMAGE LAB_AVD LAB_SDK
# The emulator, adb and avdmanager read these. ANDROID_USER_HOME also holds the AGP debug keystore.
export ANDROID_SDK_ROOT="$LAB_SDK"
export ANDROID_HOME="$LAB_SDK"
export ANDROID_AVD_HOME="$LAB_HOME/avd"
export ANDROID_USER_HOME="$LAB_HOME/user"
export ANDROID_EMULATOR_HOME="$LAB_HOME/user"
export GRADLE_USER_HOME="$LAB_HOME/gradle"
export TMPDIR="$LAB_HOME/tmp/"
LAB_ADB="$LAB_SDK/platform-tools/adb"
LAB_EMULATOR="$LAB_SDK/emulator/emulator"
LAB_SDKMANAGER="$LAB_SDK/cmdline-tools/latest/bin/sdkmanager"
LAB_AVDMANAGER="$LAB_SDK/cmdline-tools/latest/bin/avdmanager"
export LAB_ADB LAB_EMULATOR LAB_SDKMANAGER LAB_AVDMANAGER
# The lab emulator has a fixed console port, so every adb call names its serial and can never
# reach another device or emulator on this machine.
: "${LAB_PORT:=5600}"
LAB_SERIAL="emulator-$LAB_PORT"
export LAB_PORT LAB_SERIAL
# Harness app (the conductor default) and its instrumentation runner.
LAB_PACKAGE=org.rivqen.lab.harness
LAB_RUNNER="$LAB_PACKAGE/org.rivqen.lab.harness.LabInstrumentation"
export LAB_PACKAGE LAB_RUNNER
