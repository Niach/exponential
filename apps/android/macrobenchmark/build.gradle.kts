// EXP-1521/1522: frame-timing Macrobenchmarks for the session transcript
// scroll and the send-then-back journey, measured against the app's
// `benchmark` build type (R8 release, profileable, debug-signed). They need a
// device or emulator and a signed-in instance holding a long session; see
// README.md. Not part of the PR gate (no device there).
plugins {
    alias(libs.plugins.android.test)
}

android {
    namespace = "com.exponential.macrobenchmark"
    compileSdk = 37

    defaultConfig {
        minSdk = 28
        targetSdk = 36
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        // Emulator runs are allowed (the numbers compare before/after on the
        // same device, never across devices).
        testInstrumentationRunnerArguments["androidx.benchmark.suppressErrors"] = "EMULATOR"
    }

    buildTypes {
        create("benchmark") {
            isDebuggable = true
            signingConfig = signingConfigs.getByName("debug")
            matchingFallbacks += listOf("release")
        }
    }

    flavorDimensions += "env"
    productFlavors {
        create("production") { dimension = "env" }
        create("staging") { dimension = "env" }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    targetProjectPath = ":app"
    experimentalProperties["android.experimental.self-instrumenting"] = true
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

androidComponents {
    beforeVariants(selector().all()) { it.enable = it.buildType == "benchmark" }
}

dependencies {
    implementation(libs.androidx.test.junit)
    implementation(libs.benchmark.macro.junit4)
    implementation(libs.uiautomator)
}
