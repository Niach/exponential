// #VAPP-270 — the Android RUNNER's licence report, without touching its build.
//
// apps/runner/android is the template APK `exponential bundle` patches; its
// build files belong to the runner. This init script applies the same
// `com.jaredsburrows.license` plugin the app uses (apps/android, version from
// apps/android/gradle/libs.versions.toml `license-plugin`) to the runner's
// `:app` only, JSON only, nothing copied into assets/:
//
//   packages/exponential-ui-compose/gradlew -p apps/runner/android \
//     --init-script packages/licenses/gradle/runner-license.init.gradle.kts licenseReleaseReport
//   bun run --filter @exp/licenses collect:android -- --runner
//
// The report is the resolved `releaseRuntimeClasspath`: the Compose BOM's
// expansion, JNA, and what the included builds (eui-compose, the Engine AAR)
// pull in transitively.
// The plugin goes on `:app`'s OWN buildscript classpath (not the init
// script's): it links against AGP's API, which only that classloader sees.
// Init scripts run for every build, the included ones too: hook ONLY the
// runner's (a hook in eui-compose's build fails its lazy configuration).
gradle.settingsEvaluated {
    if (rootProject.name != "exponential-runner") return@settingsEvaluated
    gradle.beforeProject { runnerApp() }
    // The plugin resolves `releaseRuntimeClasspath` while its task is CREATED
    // (a lazy `tasks.register`), and the first resolve loads the included
    // builds, which Gradle refuses inside a lazy context ("cannot be executed
    // in the current context"). Resolve it once, eagerly, first: in `:app`'s
    // own afterEvaluate (Gradle 9 refuses a resolve from `projectsEvaluated`,
    // which holds no project lock).
}

fun Project.runnerApp() {
    if (path != ":app") return
    buildscript.repositories.gradlePluginPortal()
    buildscript.dependencies.add("classpath", "com.jaredsburrows:gradle-license-plugin:0.9.9")
    pluginManager.withPlugin("com.android.application") {
        afterEvaluate { configurations.getByName("releaseRuntimeClasspath").incoming.resolutionResult.root }
        apply(plugin = "com.jaredsburrows.license")
        extensions.getByName("licenseReport").withGroovyBuilder {
            for (flag in listOf("GenerateCsvReport", "GenerateHtmlReport", "GenerateTextReport",
                "CopyCsvReportToAssets", "CopyHtmlReportToAssets", "CopyJsonReportToAssets", "CopyTextReportToAssets")) {
                "set$flag"(false)
            }
            "setGenerateJsonReport"(true)
        }
    }
}
