// A fresh Android app on the Exponential UI SDK: `at.exponential:ui-compose`
// from Maven Central (mavenLocal() first, so the guide check builds against
// a local publish of the checkout).
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
