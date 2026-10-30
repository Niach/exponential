plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.ksp)
    alias(libs.plugins.hilt)
    alias(libs.plugins.google.services)
    alias(libs.plugins.license)
}

// EXP-375 third-party notices. `licenseProductionReleaseReport` resolves the
// shipped runtime graph — which is what the BOMs actually expand to, not the ~49
// aliases in libs.versions.toml — and writes
// app/build/reports/licenses/licenseProductionReleaseReport.json.
// packages/licenses/scripts/collect-android.ts parses that into
// packages/licenses/inventory/android.json.
//
// JSON only, and NOTHING is copied into assets/: the merge step owns
// app/src/main/assets/NOTICES.txt and a plugin writing there would fight the
// drift gate.
licenseReport {
    generateCsvReport = false
    generateHtmlReport = false
    generateJsonReport = true
    copyCsvReportToAssets = false
    copyHtmlReportToAssets = false
    copyJsonReportToAssets = false
}

// Hilt's annotation processor parses @Metadata off every annotation it sees on an
// @AndroidEntryPoint class — kotlin.OptIn among them — and hilt 2.58 pins
// kotlin-metadata-jvm 2.2.20, which hard-refuses metadata newer than 2.3.0.
// coil 3.5.0 floors kotlin-stdlib at 2.4.0, so those stdlib annotation classes
// carry metadata 2.4.0 and Hilt aborts with "update the kotlin-metadata-jvm
// library". Doing exactly that, pinned to the toolchain's own Kotlin version.
// Drop this once Hilt ships a build that both reads 2.4 metadata and supports
// AGP 8 (2.59+ requires AGP 9).
configurations.configureEach {
    resolutionStrategy.force("org.jetbrains.kotlin:kotlin-metadata-jvm:${libs.versions.kotlin.get()}")
}

// Release signing is fed by gradle properties or environment variables so CI can inject a
// keystore without committing it. When RELEASE_STORE_FILE is absent (e.g. pre-keystore CI or
// local dev) the release build stays UNSIGNED — assembleRelease keeps working, so the pipeline
// stays green until a keystore exists. Generate the upload keystore
// once with keytool (RSA 2048, alias `upload`) and keep it outside the repo.
fun releaseProp(name: String): String? =
    (project.findProperty(name) as String?) ?: System.getenv(name)

val releaseStoreFile = releaseProp("RELEASE_STORE_FILE")
val releaseStorePassword = releaseProp("RELEASE_STORE_PASSWORD")
val releaseKeyAlias = releaseProp("RELEASE_KEY_ALIAS")
val releaseKeyPassword = releaseProp("RELEASE_KEY_PASSWORD")

// VAPP-40 / EXP-1457: ONE eNet library, the FULL `exponential-net-ffi` node
// (iroh + chat + the push functions): the app's per-account nodes and the FCM
// service's `pushOpen` (EXP-1435) load the same `libexponential_net_ffi.so`.
// The push-only build is not linked any more: same library and package names,
// so the two could never ship side by side anyway, and an FCM service runs in
// the app process (no NSE-style memory cap as on iOS). The Kotlin binding is a
// COMMITTED source; the `.so` per ABI is built by `buildEnetFfi` (cargo-ndk,
// below) into out/jniLibs (gitignored). A build without it still runs: every
// facade call is guarded (the FCM service registers its bare token, the
// nodes stay off).
val repoRoot: File = rootDir.parentFile.parentFile
val enetFfiCrate = File(repoRoot, "apps/desktop/crates/exponential-net-ffi")
/** The host library the JVM unit tests load through JNA (`cargo build -p exponential-net-ffi --release --features cli`). */
val cargoTarget = System.getenv("CARGO_TARGET_DIR")?.let(::File) ?: File(repoRoot, "apps/desktop/target")
val cargoNdk: File? = (System.getenv("PATH").orEmpty().split(File.pathSeparator).map { File(it, "cargo-ndk") } +
    File(System.getProperty("user.home"), ".cargo/bin/cargo-ndk")).firstOrNull { it.canExecute() }

// cargo-ndk before merge*JniLibFolders: arm64-v8a, armeabi-v7a, x86_64 with the
// `mobile` profile (LTO, stripped, 16 KB-aligned LOAD segments). Up to date while
// the eNet sources and Cargo.lock are unchanged. ENET_SKIP_NATIVE=1 (or no
// cargo-ndk on this machine) skips it and keeps whatever out/jniLibs holds.
val buildEnetFfi = tasks.register<Exec>("buildEnetFfi") {
    description = "Builds the eNet node .so per ABI (apps/desktop/crates/exponential-net-ffi/build-android.sh)."
    val desktop = File(repoRoot, "apps/desktop")
    inputs.dir(File(enetFfiCrate, "src"))
    inputs.dir(File(desktop, "crates/enet-core/src"))
    inputs.files(File(enetFfiCrate, "build-android.sh"), File(enetFfiCrate, "Cargo.toml"), File(desktop, "crates/enet-core/Cargo.toml"), File(desktop, "Cargo.lock"))
    outputs.dir(File(enetFfiCrate, "out/jniLibs"))
    onlyIf("cargo-ndk is installed and ENET_SKIP_NATIVE is unset") {
        // CI never ships an app without its eNet node.
        if (cargoNdk == null && System.getenv("CI") == "true") throw GradleException("cargo-ndk not found: CI must build the eNet .so")
        System.getenv("ENET_SKIP_NATIVE") != "1" && cargoNdk != null
    }
    environment("SKIP_BINDINGS", "1")
    environment("PATH", listOfNotNull(cargoNdk?.parent, System.getenv("PATH")).joinToString(File.pathSeparator))
    commandLine("bash", File(enetFfiCrate, "build-android.sh").absolutePath)
}
tasks.matching { it.name.startsWith("merge") && it.name.endsWith("JniLibFolders") }.configureEach { dependsOn(buildEnetFfi) }

// VAPP-110: the app host's assets = the shared package's `extension.json`
// alone (its `templates/` are SDK samples, not built-in apps).
val appHostAssetsDir = layout.buildDirectory.dir("generated/appHostAssets")
val syncAppHostAssets = tasks.register<Sync>("syncAppHostAssets") {
    from("../../../packages/ui/exponential-ui") { include("extension.json") }
    into(appHostAssetsDir)
}
tasks.matching { it.name == "preBuild" }.configureEach { dependsOn(syncAppHostAssets) }

android {
    namespace = "com.exponential.app"
    compileSdk = 36

    testOptions {
        // JVM unit tests exercise classes that log via android.util.Log
        // (ShapeClient error paths); return defaults instead of throwing
        // "not mocked".
        unitTests.isReturnDefaultValues = true
        unitTests.all {
            // PushSealFixtureTest (EXP-1435) + the eNet install tests (EXP-1457): the
            // facade's host build + the shared vectors.
            it.systemProperty("jna.library.path", File(cargoTarget, "release").absolutePath)
            it.systemProperty("exponential.push.sealFixture", File(repoRoot, "packages/domain-contract/fixtures/push-seal.json").absolutePath)
            it.systemProperty("exponential.enet.installFixture", File(repoRoot, "packages/domain-contract/fixtures/install-register.json").absolutePath)
            // EXP-1459: the sealed-content vectors (SealedContentFixtureTest).
            it.systemProperty("exponential.enet.sealedFixture", File(repoRoot, "packages/domain-contract/fixtures/sealed-content.json").absolutePath)
            // VAPP-33: the steer-over-eNet envelope rules (SteerEnetTest).
            it.systemProperty("exponential.steer.enetFixture", File(repoRoot, "packages/domain-contract/fixtures/steer-enet.json").absolutePath)
        }
    }

    sourceSets {
        getByName("main") {
            kotlin.srcDir(File(enetFfiCrate, "bindings/kotlin"))
            jniLibs.srcDir(File(enetFfiCrate, "out/jniLibs"))
        }
    }

    defaultConfig {
        applicationId = "at.exponential"
        minSdk = 26
        targetSdk = 36
        versionCode = 161
        versionName = "0.14.78"

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
            // EXP-395: without R8 the three classes.dex files total ~33MB
            // uncompressed — that IS the install size users see. Shrinking
            // cuts dex to roughly a third; the R8 mapping rides the AAB
            // metadata so Play retraces crashes automatically.
            isMinifyEnabled = true
            isShrinkResources = true
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

    // VAPP-110: the app host's extension catalog (`extension.json`), byte for
    // byte what the web and desktop hosts install, synced from the shared
    // package (never copied into src/main/assets). Exponential ships no
    // built-in apps, so the package's `templates/` stay out of the APK.
    sourceSets["main"].assets.srcDir(appHostAssetsDir.get().asFile)

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
            // Host of the autoVerify https intent-filter (EXP-92 App Links);
            // must stay in lockstep with DEFAULT_CLOUD_URL.
            manifestPlaceholders["appLinkHost"] = "app.exponential.at"
        }
        create("staging") {
            dimension = "env"
            applicationIdSuffix = ".staging"
            versionNameSuffix = "-staging"
            resValue("string", "app_name", "Exp Staging")
            buildConfigField("String", "DEFAULT_CLOUD_URL", "\"https://next.exponential.at\"")
            buildConfigField("boolean", "IS_STAGING", "true")
            manifestPlaceholders["appLinkHost"] = "next.exponential.at"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
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

// Kotlin 2.4 removed the `android { kotlinOptions { } }` shim; the jvmTarget now
// lives in the KGP compilerOptions DSL and must stay in lockstep with the
// compileOptions source/target above (17).
kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

dependencies {
    implementation(project(":ui-compose-primitives"))
    // EXP-1280: the plan card renders an agent-authored eUI surface inline.
    implementation(project(":ui-compose"))
    implementation(libs.core.ktx)
    implementation(libs.core.splashscreen)
    implementation(libs.lifecycle.runtime.ktx)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.lifecycle.process)
    implementation(libs.activity.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.graphics)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    implementation(libs.navigation.compose)

    implementation(libs.hilt.android)
    ksp(libs.hilt.compiler)
    implementation(libs.hilt.navigation.compose)

    implementation(libs.ktor.client.core)
    implementation(libs.ktor.client.okhttp)
    implementation(libs.okhttp)
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
    // EXP-857: passkey login via CredentialManager (+ the Play Services
    // provider that serves the WebAuthn assertion below API 34).
    implementation(libs.credentials)
    implementation(libs.credentials.play.services.auth)
    implementation(libs.commonmark.core)
    implementation(libs.commonmark.ext.strikethrough)
    implementation(libs.commonmark.ext.tables)
    implementation(libs.coil.compose)
    implementation(libs.coil.network.ktor3)
    // EXP-824: inline video/audio playback + the upload-time 720p transcode.
    implementation(libs.media3.exoplayer)
    implementation(libs.media3.ui)
    implementation(libs.media3.transformer)
    implementation(libs.media3.effect)
    // EXP-1162: the Work screen's header + bottom-bar backdrop blur.
    implementation(libs.haze)

    implementation(platform(libs.firebase.bom))
    implementation(libs.firebase.messaging)
    // EXP-1435 / VAPP-40: the eNet facade is reached over JNA (as :ui-compose's is).
    implementation("${libs.jna.aar.get()}@aar")

    // Play in-app updates — drives the immediate update flow from the EXP-104
    // "Update required" gate on production builds.
    implementation(libs.play.app.update.ktx)

    debugImplementation(libs.compose.ui.tooling)
    // ui-test-manifest contributes the activity used by createComposeRule; harmless
    // in normal debug builds, required for the screengrab instrumentation run.
    debugImplementation(libs.compose.ui.test.manifest)

    testImplementation(libs.junit)
    testImplementation(libs.ktor.client.mock)
    testImplementation(libs.jna.aar)
    androidTestImplementation(libs.androidx.test.junit)
    androidTestImplementation(libs.androidx.test.rules)
    androidTestImplementation(libs.espresso.core)
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    // fastlane screengrab: Screengrab.screenshot() + LocaleTestRule for the
    // automated Play Store screenshot run (fastlane/Screengrabfile).
    androidTestImplementation(libs.screengrab)
}
