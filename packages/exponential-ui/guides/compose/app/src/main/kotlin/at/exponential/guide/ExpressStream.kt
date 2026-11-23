package at.exponential.guide

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import at.exponential.ui.ExponentialUICatalog
import at.exponential.ui.host.A2UI_VERSION
import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.HostSurface
import at.exponential.ui.host.InferenceFormat
import at.exponential.ui.host.SseTransport
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import org.json.JSONArray
import org.json.JSONObject

/** The prose is yours: the SDK hands it to `onText` and draws none of it. Here it goes into two surfaces the app owns (a Markdown each): the run before the block above the agent's surface, the run after it below. */
class ProseSurfaces {
    var host: ExponentialHost? = null
    private var target: String? = null
    private var run = StringBuilder()

    fun piece(piece: String) {
        val host = host ?: return
        if (target == null) {
            target = "intro"
            for (surfaceId in listOf("intro", "outro")) {
                val root = JSONObject().put("id", "root").put("component", "Markdown").put("text", JSONObject().put("@path", "/text"))
                val create = JSONObject().put("surfaceId", surfaceId).put("catalogId", ExponentialUICatalog.catalogId)
                    .put("components", JSONArray().put(root)).put("dataModel", JSONObject().put("text", ""))
                host.receive(JSONObject().put("version", A2UI_VERSION).put("createSurface", create).toString())
            }
        }
        // A "\n\n" piece = a block passed: the next run starts.
        if (piece == "\n\n") {
            target = "outro"
            run = StringBuilder()
            return
        }
        run.append(piece)
        val update = JSONObject().put("surfaceId", target).put("path", "/text").put("value", run.toString())
        host.receive(JSONObject().put("version", A2UI_VERSION).put("updateDataModel", update).toString())
    }
}

/** The Express stream: a model's answer written in Express (prose around an `<a2ui>` block); guides/agent `GET /express` replays one. */
fun expressHost(): ExponentialHost {
    val prose = ProseSurfaces()
    val host = ExponentialHost(
        HostOptions(
            // `InferenceFormat.Express`: the agent writes Express (prose around `<a2ui>` blocks).
            // `reconnectMs = 0`: the answer ends, nothing to resume.
            transport = SseTransport(url = "http://10.0.2.2:4300/express", postUrl = "http://10.0.2.2:4300/action", reconnectMs = 0, format = InferenceFormat.Express),
            theme = ThemeHandle.builtin("exponential"),
            mode = Mode.Dark,
            onText = prose::piece,
        ),
    )
    prose.host = host
    return host
}

@Composable
fun ExpressStream(host: ExponentialHost) {
    // The page behind the dark surfaces: the theme's dark `color.background` (as on the web guides).
    Column(Modifier.fillMaxSize().background(Color(0xFF0A0A0A)).safeDrawingPadding().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        HostSurface(host, "intro")
        HostSurface(host, "main")
        HostSurface(host, "outro")
    }
}
