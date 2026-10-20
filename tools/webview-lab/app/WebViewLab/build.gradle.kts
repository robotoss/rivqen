// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

buildscript {
    dependencies {
        // AGP 9 compiles Kotlin with its own Kotlin Gradle plugin (2.2.10 for AGP 9.1.1).
        // This line pins the Kotlin version of the lab (CONTRACT section 1).
        classpath(libs.kotlin.gradlePlugin)
    }
}

plugins {
    // Load the plugin once, in the root class loader.
    alias(libs.plugins.androidApplication) apply false
}
