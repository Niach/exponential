// #EXP-1441: `at.exponential:etalk`, eTalk for Kotlin (Android): the UniFFI
// facade etalk-ffi as one AAR (the `.so` per ABI + the committed binding).
// The toolchain is eUI Compose's (its version catalog and Gradle wrapper):
//   ../../../../../packages/exponential-ui-compose/gradlew -p . <task>
// release/central-bundle.sh is the Maven Central upload bundle.
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
            from(files("../../../../../packages/exponential-ui-compose/gradle/libs.versions.toml"))
        }
    }
}

rootProject.name = "etalk"
