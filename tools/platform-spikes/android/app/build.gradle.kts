// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import org.jetbrains.kotlin.gradle.dsl.JvmTarget

// The Android prototype A-STREAM of WP-04 (tools/platform-spikes/CONTRACT.md): early fetch with
// OkHttp, main-frame shouldInterceptRequest with a bridge pipe, probes through addWebMessageListener.
plugins {
    alias(libs.plugins.androidApplication)
}

/**
 * Copies the lab CA ($LAB_TLS/ca.pem, CONTRACT.md section 2) into a generated raw resource of the
 * debug build. The debug network security config names it in a debug-overrides trust anchor; the
 * release build has neither the file nor the override.
 */
abstract class LabCaResources : DefaultTask() {
    @get:Input
    abstract val tlsDir: Property<String>

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    @TaskAction
    fun copy() {
        val ca = File(tlsDir.get(), "ca.pem")
        if (!ca.isFile) {
            throw GradleException("no lab CA at $ca: start the spike host once (it makes the TLS files)")
        }
        val raw = outputDir.get().dir("raw").asFile
        raw.deleteRecursively()
        if (!raw.mkdirs()) throw GradleException("cannot create $raw")
        ca.copyTo(File(raw, "lab_ca.pem"), overwrite = true)
    }
}

val labCa = tasks.register<LabCaResources>("labCaResources") {
    tlsDir.set(providers.environmentVariable("LAB_TLS").orElse(""))
    outputDir.set(layout.buildDirectory.dir("generated/labCa"))
}

androidComponents {
    onVariants(selector().withBuildType("debug")) { variant ->
        variant.sources.res?.addGeneratedSourceDirectory(labCa, LabCaResources::outputDir)
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_11
        allWarningsAsErrors = true
    }
}

dependencies {
    implementation(libs.androidx.webkit)
    implementation(libs.okhttp)
}

android {
    namespace = "dev.rivqen.spike"
    compileSdk = libs.versions.android.compileSdk.get().toInt()

    defaultConfig {
        applicationId = "dev.rivqen.spike"
        minSdk = libs.versions.android.minSdk.get().toInt()
        targetSdk = libs.versions.android.targetSdk.get().toInt()
        versionCode = 1
        versionName = "0.1"
    }
    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }
    lint {
        warningsAsErrors = true
        abortOnError = true
        // The lab pins its versions on purpose (DL-137); a newer release is not an error.
        disable += setOf("GradleDependency", "NewerVersionAvailable", "AndroidGradlePluginVersion", "OldTargetApi")
    }
}
