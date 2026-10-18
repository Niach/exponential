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

// EXP-1280: the Compose painter itself (the plan card's inline Exponential UI
// surface): the UniFFI core rides as `libexponential_ui_ffi.so` (built by
// apps/desktop/crates/exponential-ui-ffi/build-android.sh, gitignored) + JNA.
include(":ui-compose")
project(":ui-compose").projectDir = file("../../packages/exponential-ui-compose/ui-compose")
