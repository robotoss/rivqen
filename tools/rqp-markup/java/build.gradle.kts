// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import net.ltgt.gradle.errorprone.CheckSeverity
import net.ltgt.gradle.errorprone.errorprone

plugins {
    application
    id("net.ltgt.errorprone") version "5.1.1"
    id("com.diffplug.spotless") version "8.10.4"
    id("info.solidsoft.pitest") version "1.19.0"
}

group = "dev.rivqen.tools"
version = "0.0.0"

java {
    toolchain { languageVersion = JavaLanguageVersion.of(21) }
}

dependencies {
    // Annotations only; nothing is on the runtime class path.
    compileOnly("org.jspecify:jspecify:1.0.1")
    testCompileOnly("org.jspecify:jspecify:1.0.1")
    errorprone("com.google.errorprone:error_prone_core:2.50.0")
    errorprone("com.uber.nullaway:nullaway:0.14.2")

    testImplementation(platform("org.junit:junit-bom:5.14.4"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.withType<JavaCompile>().configureEach {
    options.encoding = "UTF-8"
    options.release = 21
    options.compilerArgs.addAll(listOf("-Xlint:all", "-Werror"))
    options.errorprone {
        disableWarningsInGeneratedCode = true
        check("NullAway", CheckSeverity.ERROR)
        option("NullAway:OnlyNullMarked", "true")
    }
}

tasks.named<JavaCompile>("compileTestJava") {
    options.errorprone {
        // Tests use assertThrows and similar patterns that NullAway reports without value.
        check("NullAway", CheckSeverity.OFF)
    }
}

application {
    applicationName = "rqp-markup"
    mainClass = "dev.rivqen.tools.markup.Main"
    // The input is at most 5 MiB; the heap bound keeps a defect from using the whole machine.
    applicationDefaultJvmArgs = listOf("-Xmx512m", "-Xss1m", "-XX:+UseSerialGC", "-XX:TieredStopAtLevel=1")
}

tasks.test {
    useJUnitPlatform()
    maxHeapSize = "1g"
    testLogging {
        events("failed")
        exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL
    }
}

spotless {
    java {
        googleJavaFormat("1.35.0")
        licenseHeader("// SPDX-License-Identifier: Apache-2.0\n// SPDX-FileCopyrightText: The Rivqen Authors\n\n")
    }
}

pitest {
    pitestVersion = "1.30.0"
    junit5PluginVersion = "1.2.3"
    // Override with -Ppitest.targetClasses=… (globs from tools/mutation/diff-ranges.mjs).
    targetClasses = providers.gradleProperty("pitest.targetClasses")
        .map { it.split(",").map(String::trim).filter(String::isNotEmpty) }
        .orElse(listOf("dev.rivqen.tools.markup.*"))
    targetTests = setOf("dev.rivqen.tools.markup.*")
    threads = 4
    outputFormats = setOf("HTML", "XML")
    timestampedReports = false
    mutationThreshold = 80
    jvmArgs = listOf("-Xmx1g")
}
