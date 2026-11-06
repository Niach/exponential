// #VAPP-267 (E6b): the Android RUNNER, the prebuilt APK/AAB template an
// exported app runs in (runner.json, docs/spec/apps.md §13.3). It is built
// from the workspace, never from published packages: genUI's Compose painter
// and the Engine come in as INCLUDED BUILDS (their own root dirs, so their
// relative paths keep working), substituted for their Maven coordinates.
// The toolchain is eUI Compose's (its version catalog and Gradle wrapper):
//   ../../../packages/exponential-ui-compose/gradlew -p . <task>
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
    versionCatalogs {
        create("libs") {
            from(files("../../../packages/exponential-ui-compose/gradle/libs.versions.toml"))
        }
    }
}

rootProject.name = "exponential-runner"
include(":app")

// genUI's Compose painter (`at.exponential:eui-compose`, packages/exponential-ui-compose).
includeBuild("../../../packages/exponential-ui-compose") {
    dependencySubstitution {
        substitute(module("at.exponential:eui-compose")).using(project(":ui-compose"))
    }
}
// The Engine (`at.exponential:engine`: eengine-ffi's AAR + the E5 capabilities).
includeBuild("../../desktop/crates/eengine-ffi/android") {
    dependencySubstitution {
        substitute(module("at.exponential:engine")).using(project(":"))
    }
}
