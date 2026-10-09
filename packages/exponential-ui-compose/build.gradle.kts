import org.gradle.api.publish.PublishingExtension
import org.gradle.api.publish.maven.MavenPublication
import org.gradle.plugins.signing.SigningExtension

plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.library) apply false
    alias(libs.plugins.kotlin.android) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.roborazzi) apply false
}

// VAPP-91: Maven Central for `at.exponential:ui-compose` and
// `at.exponential:ui-compose-primitives`. Every publishing module gets the
// shared POM (licence, developers, scm), the local `centralBundle`
// repository (`build/central-bundle`, zipped by release/central-bundle.sh
// into the Central Portal upload) and, ONLY when a key is provided
// (`ORG_GRADLE_PROJECT_signingInMemoryKey` / `…Password`), signing. The
// version is the `uiVersion` Gradle property (default 0.1.0, read by the
// modules themselves so `:ui-compose-primitives` keeps it when the
// Exponential Android app includes it by path).
val signingKey = providers.gradleProperty("signingInMemoryKey").orNull
val signingPassword = providers.gradleProperty("signingInMemoryKeyPassword").orNull
val centralBundle = layout.buildDirectory.dir("central-bundle")

subprojects {
    pluginManager.withPlugin("maven-publish") {
        extensions.configure<PublishingExtension> {
            repositories {
                maven {
                    name = "centralBundle"
                    url = uri(centralBundle)
                }
            }
            publications.withType<MavenPublication>().configureEach {
                pom {
                    url.set("https://github.com/Niach/exponential")
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
        if (signingKey != null) {
            apply(plugin = "signing")
            extensions.configure<SigningExtension> {
                useInMemoryPgpKeys(signingKey, signingPassword)
                sign(extensions.getByType<PublishingExtension>().publications)
            }
        }
    }
}
