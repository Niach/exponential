plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.ksp)
    alias(libs.plugins.hilt)
    alias(libs.plugins.google.services)
}

// Release signing is fed by gradle properties or environment variables so CI can inject a
// keystore without committing it. When RELEASE_STORE_FILE is absent (e.g. pre-keystore CI or
// local dev) the release build stays UNSIGNED — assembleRelease keeps working, so the pipeline
// stays green until a keystore exists. See docs/release-android.md for keystore generation.
fun releaseProp(name: String): String? =
    (project.findProperty(name) as String?) ?: System.getenv(name)

val releaseStoreFile = releaseProp("RELEASE_STORE_FILE")
val releaseStorePassword = releaseProp("RELEASE_STORE_PASSWORD")
val releaseKeyAlias = releaseProp("RELEASE_KEY_ALIAS")
val releaseKeyPassword = releaseProp("RELEASE_KEY_PASSWORD")

android {
    namespace = "com.exponential.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "at.exponential"
        minSdk = 26
        targetSdk = 35
        versionCode = 25
        versionName = "0.8.9"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables {
            useSupportLibrary = true
        }
    }

    signingConfigs {
        // Only materialize a release keystore when one is provided; otherwise release
        // builds fall through to an unsigned artifact (still installable via `adb`, and
        // signable/uploadable later — the Play Console can accept an upload key at first push).
        if (releaseStoreFile != null) {
            create("release") {
                storeFile = file(releaseStoreFile)
                storePassword = releaseStorePassword
                keyAlias = releaseKeyAlias
                keyPassword = releaseKeyPassword
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (releaseStoreFile != null) {
                signingConfig = signingConfigs.getByName("release")
            }
            // Package native symbol tables into the AAB so Play can symbolicate
            // crashes/ANRs in dependency .so libs (stripped before delivery).
            ndk {
                debugSymbolLevel = "SYMBOL_TABLE"
            }
        }
        debug {
            // Same applicationId as release so a single google-services.json
            // client (registered for at.exponential) covers both.
            isDebuggable = true
        }
    }

    // Product flavors mirror the iOS Tuist targets (Exponential / Exponential-Staging):
    // `production` ships the public cloud default, `staging` co-installs under a
    // `.staging` applicationId and defaults to next.exponential.at. The default
    // cloud URL + a staging flag are injected via BuildConfig (read by
    // AppConstants), and the launcher label via a per-flavor app_name resValue.
    flavorDimensions += "env"
    productFlavors {
        create("production") {
            dimension = "env"
            resValue("string", "app_name", "Exponential")
            buildConfigField("String", "DEFAULT_CLOUD_URL", "\"https://app.exponential.at\"")
            buildConfigField("boolean", "IS_STAGING", "false")
        }
        create("staging") {
            dimension = "env"
            applicationIdSuffix = ".staging"
            versionNameSuffix = "-staging"
            resValue("string", "app_name", "Exp Staging")
            buildConfigField("String", "DEFAULT_CLOUD_URL", "\"https://next.exponential.at\"")
            buildConfigField("boolean", "IS_STAGING", "true")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    buildFeatures {
        compose = true
        buildConfig = true
    }
    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

dependencies {
    implementation(libs.core.ktx)
    implementation(libs.lifecycle.runtime.ktx)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.activity.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.graphics)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons)
    implementation(libs.navigation.compose)

    implementation(libs.hilt.android)
    ksp(libs.hilt.compiler)
    implementation(libs.hilt.navigation.compose)

    implementation(libs.ktor.client.core)
    implementation(libs.ktor.client.cio)
    implementation(libs.ktor.client.content.negotiation)
    implementation(libs.ktor.serialization.kotlinx.json)
    implementation(libs.ktor.client.auth)
    implementation(libs.ktor.client.logging)
    implementation(libs.ktor.client.websockets)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.kotlinx.coroutines.core)
    implementation(libs.kotlinx.coroutines.android)

    implementation(libs.room.runtime)
    implementation(libs.room.ktx)
    ksp(libs.room.compiler)

    implementation(libs.datastore.preferences)
    implementation(libs.security.crypto)
    implementation(libs.browser)
    implementation(libs.commonmark.core)
    implementation(libs.commonmark.ext.strikethrough)
    implementation(libs.coil.compose)
    implementation(libs.coil.network.ktor3)

    implementation(platform(libs.firebase.bom))
    implementation(libs.firebase.messaging)

    debugImplementation(libs.compose.ui.tooling)
    // ui-test-manifest contributes the activity used by createComposeRule; harmless
    // in normal debug builds, required for the screengrab instrumentation run.
    debugImplementation(libs.compose.ui.test.manifest)

    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.test.junit)
    androidTestImplementation(libs.androidx.test.rules)
    androidTestImplementation(libs.espresso.core)
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    // fastlane screengrab: Screengrab.screenshot() + LocaleTestRule for the
    // automated Play Store screenshot run (fastlane/Screengrabfile).
    androidTestImplementation(libs.screengrab)
}
