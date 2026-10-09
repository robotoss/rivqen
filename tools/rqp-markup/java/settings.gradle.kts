// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Optional mirror of Maven Central, for local runs when Central rate-limits the host:
//   ORG_GRADLE_PROJECT_rqpMavenMirror=https://<mirror>/maven2 gradle test
// Without the property, the build uses Maven Central and the Gradle Plugin Portal only.
val rqpMavenMirror: Provider<String> = providers.gradleProperty("rqpMavenMirror")

pluginManagement {
    repositories {
        val mirror = providers.gradleProperty("rqpMavenMirror")
        if (mirror.isPresent) {
            maven { url = uri(mirror.get()) }
        }
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode = RepositoriesMode.FAIL_ON_PROJECT_REPOS
    repositories {
        if (rqpMavenMirror.isPresent) {
            maven { url = uri(rqpMavenMirror.get()) }
        } else {
            mavenCentral()
        }
    }
}

rootProject.name = "rqp-markup"
