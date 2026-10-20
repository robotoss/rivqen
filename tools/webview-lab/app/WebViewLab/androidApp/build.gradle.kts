// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import org.jetbrains.kotlin.gradle.dsl.JvmTarget

// The Android app of the WebView lab (tools/webview-lab/CONTRACT.md, WP-17 S5 T-04).
// A plain Activity with android.webkit.WebView.
plugins {
    alias(libs.plugins.androidApplication)
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_11
        allWarningsAsErrors = true
    }
}

dependencies {
    // Isolated-world JavaScript (WebViewCompat.getExecutionWorld, JavaScriptReplyProxy.executeJavaScript).
    implementation(libs.androidx.webkit)
    implementation(libs.kotlinx.coroutines.android)
}

android {
    namespace = "dev.rivqen.webviewlab"
    compileSdk = libs.versions.android.compileSdk.get().toInt()

    defaultConfig {
        applicationId = "dev.rivqen.webviewlab"
        minSdk = libs.versions.android.minSdk.get().toInt()
        targetSdk = libs.versions.android.targetSdk.get().toInt()
        versionCode = 1
        versionName = "1.0"
    }
    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }
    lint {
        warningsAsErrors = true
        abortOnError = true
        // The lab pins its versions on purpose (CONTRACT section 1); a newer release is not an error.
        disable += setOf("GradleDependency", "NewerVersionAvailable", "AndroidGradlePluginVersion", "OldTargetApi")
    }
}
