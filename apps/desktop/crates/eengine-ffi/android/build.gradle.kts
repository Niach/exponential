// #VAPP-263: `at.exponential:engine`. One AAR = eengine-ffi's `.so` for
// arm64-v8a, armeabi-v7a and x86_64 (built by ../build-android.sh into
// ../out/jniLibs) + the generated Kotlin binding (committed, package
// `at.exponential.engine.ffi`) over JNA + the native capabilities (#VAPP-265,
// `at.exponential.engine.capabilities`). Depends on nothing from the app.
// The POM, the local `centralBundle` repository and signing (ONLY with
// `ORG_GRADLE_PROJECT_signingInMemoryKey` / `…Password`) are eUI Compose's
// (packages/exponential-ui-compose/build.gradle.kts). `-PengineVersion=…` sets
// the release version.
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.library)
    id("maven-publish")
    id("signing")
}

val ffiCrate = rootDir.parentFile

group = "at.exponential"
version = (findProperty("engineVersion") as String?) ?: "0.1.0"

android {
    namespace = "at.exponential.engine"
    compileSdk = 37

    defaultConfig {
        minSdk = 26
        consumerProguardFiles("consumer-rules.pro")
    }

    sourceSets {
        getByName("main") {
            kotlin.srcDir(File(ffiCrate, "bindings/kotlin"))
            jniLibs.srcDir(File(ffiCrate, "out/jniLibs"))
        }
    }

    // #VAPP-265: the capabilities' pure parts run on the plain JVM (no
    // Robolectric, no .so) and check their results against the contract.
    testOptions {
        unitTests.all {
            it.systemProperty("eengine.fixtures", File(ffiCrate, "../../../../packages/eengine/fixtures").canonicalPath)
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    publishing {
        singleVariant("release") {
            withSourcesJar()
            withJavadocJar()
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_17
    }
}

dependencies {
    implementation("${libs.jna.aar.get()}@aar")
    // #VAPP-265: the native capabilities (package
    // `at.exponential.engine.capabilities`): the Activity Result API for the
    // pickers and permission prompts, NotificationCompat.
    implementation("androidx.activity:activity:${libs.versions.activity.compose.get()}")
    implementation(libs.core.ktx)
    testImplementation(libs.junit)
}

publishing {
    repositories {
        maven {
            name = "centralBundle"
            url = uri(layout.buildDirectory.dir("central-bundle"))
        }
    }
    publications {
        register<MavenPublication>("release") {
            groupId = "at.exponential"
            artifactId = "engine"
            version = project.version.toString()
            afterEvaluate { from(components["release"]) }
            pom {
                name.set("Engine")
                description.set("The Engine for Kotlin: the runtime that runs Apps (instances, lifecycle, records, the outbox, consent and the capability broker) over the app's own providers, behind its UniFFI facade. Painting stays in the app.")
                url.set("https://exponential.at/sdk/apps/")
                licenses {
                    license {
                        name.set("Apache-2.0")
                        url.set("https://www.apache.org/licenses/LICENSE-2.0")
                    }
                }
                developers {
                    developer {
                        id.set("exponential")
                        name.set("Exponential")
                        email.set("hello@exponential.at")
                    }
                }
                scm {
                    url.set("https://github.com/Niach/exponential")
                    connection.set("scm:git:https://github.com/Niach/exponential.git")
                    developerConnection.set("scm:git:ssh://git@github.com/Niach/exponential.git")
                }
            }
        }
    }
}

val signingKey = providers.gradleProperty("signingInMemoryKey").orNull
if (signingKey != null) {
    signing {
        useInMemoryPgpKeys(signingKey, providers.gradleProperty("signingInMemoryKeyPassword").orNull)
        sign(publishing.publications)
    }
}
