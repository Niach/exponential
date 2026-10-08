// VAPP-91 sample: a standalone Android Studio project (NOT part of any
// monorepo build) that consumes the Exponential UI Compose SDK the way any
// app would: `at.exponential:ui-compose` from a Maven repository
// (`mavenLocal()` until it is on Maven Central).
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        mavenLocal()
        google()
        mavenCentral()
    }
}

rootProject.name = "exponential-ui-greenhouse"
include(":app")
