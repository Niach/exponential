// #EXP-1441: `at.exponential:etalk`. One AAR = etalk-ffi's `.so` for
// arm64-v8a, armeabi-v7a and x86_64 (built by ../build-android.sh into
// ../out/jniLibs) + the generated Kotlin binding (committed, package
// `at.exponential.talk.ffi`) over JNA. Depends on nothing from the app.
// The POM, the local `centralBundle` repository and signing (ONLY with
// `ORG_GRADLE_PROJECT_signingInMemoryKey` / `…Password`) are eUI Compose's
// (packages/exponential-ui-compose/build.gradle.kts). `-PtalkVersion=…` sets
// the release version.
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    id("maven-publish")
    id("signing")
}

val ffiCrate = rootDir.parentFile

group = "at.exponential"
version = (findProperty("talkVersion") as String?) ?: "0.2.2"

android {
    namespace = "at.exponential.talk"
    compileSdk = 36

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
            artifactId = "etalk"
            version = project.version.toString()
            afterEvaluate { from(components["release"]) }
            pom {
                name.set("eTalk")
                description.set("eTalk for Kotlin: the call machine (the ring, the warm-up, the agent speaks first) and the Opus codec (PCM in, packets out) behind their UniFFI facade. Audio I/O and transport stay in the app.")
                url.set("https://exponential.at/talk/")
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
