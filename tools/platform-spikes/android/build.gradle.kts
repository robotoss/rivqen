// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

buildscript {
    dependencies {
        // AGP 9 compiles Kotlin with its own Kotlin Gradle plugin; this line pins the Kotlin version
        // (the same as tools/webview-lab).
        classpath(libs.kotlin.gradlePlugin)
    }
}

plugins {
    // Load the plugin once, in the root class loader.
    alias(libs.plugins.androidApplication) apply false
}

// Build output goes to the lab root (DL-138) when LAB_HOME is set; else to build/ (git-ignored).
val labHome: String? = providers.environmentVariable("LAB_HOME").orNull
if (labHome != null) {
    allprojects {
        layout.buildDirectory.set(file("$labHome/build/platform-spikes-android/${project.name}"))
    }
}
