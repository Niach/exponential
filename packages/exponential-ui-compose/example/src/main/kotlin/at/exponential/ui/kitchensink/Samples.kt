package at.exponential.ui.kitchensink

import android.content.res.AssetManager
import android.util.Base64
import at.exponential.ui.json.JsonValue

/**
 * EXP-1654: the committed sample pictures (`packages/exponential-ui/samples`,
 * copied into the assets under `samples/`). A src `samples.json` `urls` names
 * resolves to a `data:` url of its bundled PNG, so the specimens paint a
 * picture, never fetching; any other src passes through unchanged.
 */
class Samples(private val assets: AssetManager) {
    private val urls: Map<String, String> by lazy {
        runCatching {
            val json = JsonValue.parse(assets.open("samples/samples.json").bufferedReader().use { it.readText() })
            json["urls"]?.obj?.mapNotNull { (src, file) -> file.string?.let { src to it } }?.toMap()
        }.getOrNull() ?: emptyMap()
    }
    private val cache = HashMap<String, String>()

    /** The `data:` url of the sample [src] names, else [src]. */
    fun resolve(src: String): String {
        val file = urls[src] ?: return src
        return synchronized(cache) {
            cache.getOrPut(file) {
                val bytes = assets.open("samples/$file").use { it.readBytes() }
                "data:image/png;base64," + Base64.encodeToString(bytes, Base64.NO_WRAP)
            }
        }
    }
}
