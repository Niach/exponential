// VAPP-89: the Exponential UI Compose painter (`at.exponential:ui-compose`),
// its platform-free primitives (`at.exponential:ui-compose-primitives`) and
// the kitchen-sink example app (a blank Android project that adds the
// library as a local module, what the android shot is captured from).
pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "exponential-ui-compose"
include(":ui-compose")
include(":ui-compose-primitives")
project(":ui-compose-primitives").projectDir = file("primitives")
include(":example")
