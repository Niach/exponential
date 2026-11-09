// #VAPP-267: the runner app. Compose + genUI's Compose host paint every
// pixel (runner.json: no chrome of its own); the Engine (eengine-ffi over
// JNA) runs the bundled package; the E5 capabilities are the natives. No
// account, no telemetry, no update channel: nothing here logs, reports or
// fetches a package. E7 (`exponential bundle`) replaces the placeholders
// below (applicationId, version, label, scheme) and `src/main/assets/app/`,
// and declares the package's permissions in the manifest.
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

val repoRoot = rootDir.parentFile.parentFile.parentFile
/** The host dylibs the JVM unit tests load through JNA (`cargo build -p eengine-ffi --release --features cli`). */
val cargoTarget = System.getenv("CARGO_TARGET_DIR")?.let(::File) ?: File(repoRoot, "apps/desktop/target")

/**
 * The ABIs the APK ships: those BOTH native libraries were built for
 * (`build-android.sh` → `out/jniLibs/<abi>/`), else the three they build by
 * default. JNA's AAR carries jnidispatch for seven ABIs; without the filter an
 * ABI with jnidispatch alone would install and fail to load the Engine.
 */
val nativeAbis: List<String> = run {
    val libs = listOf("eengine-ffi" to "libeengine_ffi.so", "exponential-ui-ffi" to "libexponential_ui_ffi.so")
    val built = listOf("arm64-v8a", "armeabi-v7a", "x86_64").filter { abi ->
        libs.all { (crate, so) -> File(repoRoot, "apps/desktop/crates/$crate/out/jniLibs/$abi/$so").isFile }
    }
    built.ifEmpty { listOf("arm64-v8a", "armeabi-v7a", "x86_64") }
}

/**
 * The icon registry (packages/icons → the Android app's generated Compose
 * ImageVectors, `com.exponential.app.ui.icons.ExpIcons`): ONE generated file,
 * copied into a source dir, so the runner draws the same glyphs as every
 * other Exponential surface without depending on the app.
 */
abstract class IconSource : DefaultTask() {
    @get:InputFile
    abstract val source: RegularFileProperty

    @get:OutputDirectory
    abstract val output: DirectoryProperty

    @TaskAction
    fun copy() {
        val dir = output.get().asFile
        dir.deleteRecursively()
        dir.mkdirs()
        source.get().asFile.copyTo(File(dir, "ExpIcons.generated.kt"))
    }
}

val iconSource = tasks.register<IconSource>("runnerIconSource") {
    source.set(File(repoRoot, "apps/android/app/src/main/java/com/exponential/app/ui/icons/ExpIcons.generated.kt"))
    output.set(layout.buildDirectory.dir("generated/source/runnerIcons"))
}

android {
    namespace = "at.exponential.runner"
    compileSdk = 37

    defaultConfig {
        // E7: the bundle's runner.json `id` (placeholder until bundled).
        applicationId = "at.exponential.runner.template"
        minSdk = 26
        targetSdk = 36
        // E7: from runner.json `version`.
        versionCode = 1
        versionName = "0.0.0"
        // E7: the deep-link scheme = runner.json `id` (`<id>://<screen>?<params>`).
        manifestPlaceholders["runnerScheme"] = "at.exponential.runner.template"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        ndk { abiFilters += nativeAbis }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    buildFeatures {
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
        unitTests.all {
            it.systemProperty("jna.library.path", File(cargoTarget, "release").absolutePath)
            it.systemProperty("eengine.fixtures", File(repoRoot, "packages/eengine/fixtures").absolutePath)
            it.systemProperty("runner.assets", file("src/main/assets").absolutePath)
            // One JVM per test class: a UniFFI binding keeps one callback vtable per process.
            it.forkEvery = 1
            it.testLogging.exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL
        }
    }

    packaging {
        // No diagnostics in the runner: nothing ships that only a debugger reads.
        resources.excludes += setOf("META-INF/*.version", "DebugProbesKt.bin", "kotlin-tooling-metadata.json")
    }
}

androidComponents {
    onVariants { variant ->
        variant.sources.java?.addGeneratedSourceDirectory(iconSource, IconSource::output)
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_17
    }
}

dependencies {
    // Both substituted by the included builds (settings.gradle.kts): the workspace's modules, never Maven.
    implementation("at.exponential:eui-compose:0.0.0-workspace")
    implementation("at.exponential:engine:0.0.0-workspace")
    implementation(libs.core.ktx)
    implementation(libs.activity.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.kotlinx.coroutines.android)
    implementation("${libs.jna.aar.get()}@aar")

    testImplementation(libs.junit)
    testImplementation(libs.jna.aar)
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:${libs.versions.kotlinx.coroutines.get()}")
    androidTestImplementation(libs.androidx.test.junit)
    androidTestImplementation(libs.androidx.test.rules)
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    debugImplementation(libs.compose.ui.test.manifest)
}
