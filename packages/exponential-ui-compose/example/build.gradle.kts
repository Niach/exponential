// VAPP-89: the kitchen-sink example, a BLANK Android app that adds
// `:ui-compose` as a local module and nothing else (no Exponential code).
// `shots/exponential-ui-kitchen-sink/android.webp` is captured from it.
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

val repoRoot = rootDir.parentFile.parentFile
val fixtures = File(repoRoot, "packages/exponential-ui/fixtures")
val fixtureAssets = layout.buildDirectory.dir("fixtureAssets")

// The kitchen sink + the third-party test theme ride along as assets, copied
// from the SDK's fixtures at build time (never duplicated in the repo).
val copyFixtureAssets by tasks.registering(Copy::class) {
    from(fixtures) {
        include("kitchen-sink.json", "theme-extends.json")
    }
    into(fixtureAssets)
}

android {
    namespace = "at.exponential.ui.kitchensink"
    compileSdk = 36

    defaultConfig {
        applicationId = "at.exponential.ui.kitchensink"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    sourceSets {
        getByName("main") {
            assets.srcDir(fixtureAssets)
        }
    }

    buildTypes {
        release {
            // Debug-signed, non-debuggable, R8 on: the build the perf numbers
            // and the shot come from (the VAPP-4 benchmark build type).
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    buildFeatures {
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

tasks.named("preBuild") { dependsOn(copyFixtureAssets) }

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_17
    }
}

dependencies {
    implementation(project(":ui-compose"))
    implementation(libs.core.ktx)
    implementation(libs.activity.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    // The example host's icon map only (the SDK ships no icon set).
    implementation("androidx.compose.material:material-icons-core")

    androidTestImplementation(libs.androidx.test.junit)
    androidTestImplementation(libs.androidx.test.rules)
    androidTestImplementation(libs.androidx.test.uiautomator)
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    debugImplementation(libs.compose.ui.test.manifest)
}
