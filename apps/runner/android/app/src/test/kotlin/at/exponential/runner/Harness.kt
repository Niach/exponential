package at.exponential.runner

import at.exponential.engine.ffi.Capability
import at.exponential.engine.ffi.Engine
import at.exponential.engine.ffi.EngineHost
import at.exponential.engine.ffi.EngineIdentity
import at.exponential.engine.ffi.EngineProviders
import at.exponential.engine.ffi.Vault
import at.exponential.ui.json.JsonValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import java.io.File
import java.util.concurrent.Executors

object Fixtures {
    private fun read(name: String) = JsonValue.parse(File(System.getProperty("eengine.fixtures"), name).readText())
    fun runner() = read("runner.json")
    fun lifecycle() = read("lifecycle.json")
}

class MemoryVault : Vault {
    val values = HashMap<String, String>()
    override fun `read`(`key`: String): String? = values[key]
    override fun `write`(`key`: String, `value`: String) { values[key] = value }
    override fun `delete`(`key`: String) { values.remove(key) }
}

/** The photos capability's native answer (capabilities.json `nativeOutputSchema`). */
class FakePhotos : Capability {
    var calls = 0
    override fun `name`() = "photos"
    override fun `toolsJson`() = "[]"
    override fun `call`(`tool`: String, `argumentsJson`: String): String {
        calls += 1
        return """{"structuredContent":{"photos":[]}}"""
    }
}

/** What the painter was asked to do, in order. */
class FakePainter : RunnerPainter {
    val ops = ArrayList<String>()
    val surfaces = HashMap<String, Pair<JsonValue, JsonValue?>>()
    var installed: JsonValue? = null

    override fun install(packageJson: String) {
        installed = JsonValue.parse(packageJson)
        ops.add("install")
    }

    override fun open(packageId: String, screen: String, params: Map<String, String>) {
        ops.add("open $screen ${params.toSortedMap()}")
    }

    override fun paint(surface: String, componentsJson: String, dataJson: String?) {
        surfaces[surface] = JsonValue.parse(componentsJson) to dataJson?.let(JsonValue::parse)
        ops.add("paint $surface")
    }

    override fun close(surface: String) {
        surfaces.remove(surface)
        ops.add("close $surface")
    }
}

/** A controller over the real Engine (the host dylib through JNA), painting into [painter]; main = inline. */
class Harness {
    val painter = FakePainter()
    val vault = MemoryVault()
    val photos = FakePhotos()
    /** VAPP-271: the app files the Engine writes and genUI's `app-file:` images read. */
    val blobs = RunnerBlobs(null)
    private val worker = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
    val scope = CoroutineScope(SupervisorJob() + worker)
    private var ids = 0
    val controller = RunnerController(painter, { it.run() }, worker, scope) { "id-${++ids}" }

    fun engine(config: RunnerConfig): Engine = Engine(
        EngineHost("exported", "android", "phone", "en", null, config.network, null),
        EngineIdentity.Local(RunnerController.deviceId(vault)),
        EngineProviders(vault, controller.window, blobs = blobs, capabilities = listOf(photos)),
    )

    fun start(packageJson: String): Boolean {
        val config = RunnerConfig("at.acme.album", "Album", "1.0.0", at.exponential.engine.ffi.NetworkPolicy.OFF)
        return controller.start(BundleLoad.Ready(config, packageJson), blobs, ::engine)
    }
}

val ALBUM = """
{"formatVersion":6,"id":"acme.album","name":"Album","version":"1.0.0","catalogId":"https://ui.exponential.at/catalogs/core/v1",
 "functions":["records_*","app_navigate","photos_pick"],"sources":["records:*"],
 "records":{"note":{"title":"Note","schema":{"type":"object","properties":{"text":{"type":"string"}}}}},
 "capabilities":[{"id":"photos.read","why":"Pick the photos for an album."}],
 "screens":[{"template":"albums","title":"Albums"},{"template":"pages","title":"Pages"},{"template":"page","title":"Page","nav":false}],
 "templates":{"albums":{"components":[{"id":"root","component":"Text","text":"Albums"}]},"pages":{"components":[{"id":"root","component":"Text","text":"Pages"}]},
   "page":{"components":[{"id":"root","component":"Text","text":"Page"}]}}}
"""

/** A package without screens: the shell derives one, a plain launch opens it. */
val BARE = """
{"formatVersion":6,"id":"acme.bare","name":"Bare","version":"1.0.0","catalogId":"https://ui.exponential.at/catalogs/core/v1",
 "templates":{"main":{"components":[{"id":"root","component":"Text","text":"Hi"}]}}}
"""
