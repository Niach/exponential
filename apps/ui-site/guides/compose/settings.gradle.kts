// A fresh Android app on the Exponential UI SDK: `at.exponential:ui-compose`
// from a Maven repository (mavenLocal() until it is on Maven Central).
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

rootProject.name = "exponential-ui-guide"
include(":app")
