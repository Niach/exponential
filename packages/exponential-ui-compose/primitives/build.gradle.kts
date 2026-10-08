// VAPP-89: `at.exponential:ui-compose-primitives`, the generic Compose
// primitives the catalog natives are painted with (pill, segmented control,
// field chrome, drawn switch, avatar, meter track, ring, markdown model +
// view, empty state, disclosure header), themed by `PrimitiveTokens`. Pure
// Compose, no native library: the Exponential Android app includes this
// module by path (SLOP-18 convergence) without linking the core.
//
// Dependencies are literal coordinates on purpose: this module is also a
// subproject of apps/android, whose version catalog is not this one.
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("maven-publish")
}

// The Maven coordinates (the POM maps project dependencies through them).
group = "at.exponential"
version = (findProperty("uiVersion") as String?) ?: "0.1.0"

android {
    namespace = "at.exponential.ui.primitives"
    compileSdk = 36

    defaultConfig {
        minSdk = 26
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
    implementation(platform("androidx.compose:compose-bom:2024.12.01"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-text")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")

    testImplementation("junit:junit:4.13.2")
}

publishing {
    publications {
        register<MavenPublication>("release") {
            groupId = "at.exponential"
            artifactId = "ui-compose-primitives"
            version = project.version.toString()
            afterEvaluate { from(components["release"]) }
            pom {
                name.set("Exponential UI Compose primitives")
                description.set("The generic Jetpack Compose primitives the Exponential UI Compose painter draws the catalog natives with, themed by PrimitiveTokens.")
            }
        }
    }
}
