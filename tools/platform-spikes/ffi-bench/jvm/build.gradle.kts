// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// JVM harness of the FFI benchmark (CONTRACT.md section 10). `run.sh host` builds it with
//   -PrqGenDir=<UniFFI Kotlin output>  -PrqBuildDir=<build directory in the lab root>
// so no build output goes into the repository.

import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    // Kotlin from the local toolchain (the version in the Gradle cache of the host, 2.4.20).
    kotlin("jvm") version "2.4.20"
    application
}

val genDir: String = providers.gradleProperty("rqGenDir").orNull
    ?: error("set -PrqGenDir to the UniFFI Kotlin output directory (run.sh does it)")
providers.gradleProperty("rqBuildDir").orNull?.let { layout.buildDirectory = file(it) }

// The Android form of the same JNA release (classes and libjnidispatch.so per ABI); run.sh
// android dexes its classes and pushes its arm64-v8a library. Not on the JVM classpath.
val jnaAndroid: Configuration by configurations.creating {
    isCanBeConsumed = false
    isTransitive = false
}

dependencies {
    // JNA for the UniFFI Kotlin bindings, used under its Apache-2.0 option (DL-137).
    implementation("net.java.dev.jna:jna:5.19.1")
    jnaAndroid("net.java.dev.jna:jna:5.19.1@aar")
}

// Copies the JNA AAR to <build>/jna-android/ for run.sh android.
val jnaAndroidAar by tasks.registering(Sync::class) {
    from(jnaAndroid)
    into(layout.buildDirectory.dir("jna-android"))
}
tasks.named("installDist") { dependsOn(jnaAndroidAar) }

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_21
    }
    sourceSets["main"].kotlin.srcDir(genDir)
}

java {
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}

application {
    mainClass = "dev.rivqen.spikes.ffi.MainKt"
}
