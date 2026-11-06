package at.exponential.runner

import at.exponential.engine.ffi.NetworkPolicy
import at.exponential.engine.ffi.runnerConfig
import at.exponential.ui.json.JsonValue

/** A bundle's `runner.json`, validated (runner.json `config`). */
data class RunnerConfig(val id: String, val name: String, val version: String, val network: NetworkPolicy)

/** What reading the bundle gave: the package file and its config, or why it cannot run (runner.json `failureCases`). */
sealed interface BundleLoad {
    data class Ready(val config: RunnerConfig, val packageFile: String) : BundleLoad

    /** `missing` (no package file) or `config` (runner.json has issues); `signature` / `invalid` come from the Engine's load. */
    data class Failed(val kind: String) : BundleLoad
}

/**
 * runner.json `bundle`: ONE directory `app/` holding `runner.json` and
 * `package.app.json`; on Android it is the APK's `assets/app` (E7 writes it).
 * The runner never reads the network for its package.
 */
object RunnerBundle {
    const val DIR = "app"
    const val CONFIG = "runner.json"
    const val PACKAGE = "package.app.json"

    /** [open] = a file of the bundle dir by name (null = absent). runner.json first (any issue = `config`), then the package (absent = `missing`). */
    fun read(open: (String) -> String?): BundleLoad {
        val config = open(CONFIG)?.let(::config) ?: return BundleLoad.Failed("config")
        val file = open(PACKAGE)?.takeIf { it.isNotBlank() } ?: return BundleLoad.Failed("missing")
        return BundleLoad.Ready(config, file)
    }

    /** `runner.json` text → the config, or null when it has any issue (or is not JSON). */
    fun config(text: String): RunnerConfig? {
        val parsed = runCatching { JsonValue.parse(text) }.getOrNull() ?: return null
        val out = runCatching { JsonValue.parse(runnerConfig(parsed.json)) }.getOrNull() ?: return null
        if (out["issues"]?.array?.isNotEmpty() != false) return null
        val c = out["config"]?.obj ?: return null
        val network = when (c["network"]?.string) {
            "any" -> NetworkPolicy.ANY
            "upstreamsOnly" -> NetworkPolicy.UPSTREAMS_ONLY
            else -> NetworkPolicy.OFF
        }
        return RunnerConfig(
            id = c["id"]?.string ?: return null,
            name = c["name"]?.string ?: return null,
            version = c["version"]?.string ?: return null,
            network = network,
        )
    }
}
