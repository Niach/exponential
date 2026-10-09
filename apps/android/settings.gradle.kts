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

rootProject.name = "exponential-android"
include(":app")

// VAPP-89: the Exponential UI SDK primitives, included by path (SLOP-18
// convergence: the app's generic composables wrap them).
include(":ui-compose-primitives")
project(":ui-compose-primitives").projectDir = file("../../packages/exponential-ui-compose/primitives")
