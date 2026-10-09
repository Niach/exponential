// VAPP-91 sample: a plain Android app hosting an Exponential UI surface
// streamed from the local A2UI JSONL server (samples/exponential-ui/server),
// with the server's custom theme and ONE custom extension component
// (TrendLine, painted natively below). No Exponential account, no backend
// of ours: the SDK's public API only (`ExponentialHost` + a transport +
// `HostSurface`). The Compose twin of samples/exponential-ui/web.
package at.exponential.samples.greenhouse

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import at.exponential.ui.ExponentialUi
import at.exponential.ui.extension.ExtensionContext
import at.exponential.ui.extension.ExtensionLeaf
import at.exponential.ui.extension.ExtensionPainter
import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.HostExtension
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.HostPolicy
import at.exponential.ui.host.HostSurface
import at.exponential.ui.host.JsonlStreamTransport
import at.exponential.ui.primitives.parseHexColor
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL

/**
 * Launch extras: `--es server <url>` (default `http://10.0.2.2:4190`, the
 * emulator's alias for the host machine), `--ez live true` (keep the stream
 * open for the server's live readings; default: `?once=1`, the surface
 * only), `--es mode dark`.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // The default `openUrl` opens links through this context.
        ExponentialUi.appContext = applicationContext
        val server = intent.getStringExtra("server") ?: "http://10.0.2.2:4190"
        val live = intent.getBooleanExtra("live", false)
        val mode = if (intent.getStringExtra("mode") == "dark") Mode.Dark else Mode.Light
        setContent { Greenhouse(server, live, mode) }
    }
}

/** The extension's painter: a polyline of the bound readings. */
object TrendLinePainter : ExtensionPainter {
    /** As wide as the layout offers, `height` tall (48 by default). */
    override fun measure(leaf: ExtensionLeaf, wrap: Float?): Size =
        Size(wrap ?: 240f, leaf.props["height"]?.number?.toFloat() ?: 48f)

    @Composable
    override fun Paint(context: ExtensionContext) {
        val values = context.props["values"]?.array?.mapNotNull { it.number } ?: emptyList()
        val color = context.props["color"]?.string?.let(::parseHexColor)
            ?: context.theme?.color("primary", context.mode)
            ?: context.ink
        Canvas(Modifier.fillMaxSize().semantics { contentDescription = "${values.size} readings" }) {
            if (values.size < 2) return@Canvas
            val lo = values.min()
            val hi = values.max()
            val stroke = 3.dp.toPx()
            val inset = stroke / 2
            val h = size.height - stroke
            val path = Path()
            values.forEachIndexed { i, v ->
                val x = size.width * i / (values.size - 1)
                val y = inset + if (hi == lo) h / 2 else (h * (1 - (v - lo) / (hi - lo))).toFloat()
                if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
            }
            drawPath(path, color, style = Stroke(width = stroke, cap = StrokeCap.Round, join = StrokeJoin.Round))
        }
    }
}

private class Assets(val extension: String, val theme: String)

private fun fetch(url: String): String {
    val conn = URL(url).openConnection() as HttpURLConnection
    try {
        conn.connectTimeout = 10_000
        if (conn.responseCode !in 200..299) error("$url: HTTP ${conn.responseCode}")
        return conn.inputStream.use { it.readBytes().toString(Charsets.UTF_8) }
    } finally {
        conn.disconnect()
    }
}

@Composable
private fun Greenhouse(server: String, live: Boolean, mode: Mode) {
    var assets by remember { mutableStateOf<Assets?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(server) {
        try {
            // The theme and the extension catalog come from the server at runtime.
            assets = withContext(Dispatchers.IO) { Assets(fetch("$server/extension.json"), fetch("$server/theme.json")) }
        } catch (e: Exception) {
            problem = e.message ?: e.toString()
        }
    }
    val a = assets
    if (a == null) {
        Text(problem ?: "Loading the theme and the extension from $server…", Modifier.safeDrawingPadding().padding(16.dp))
        return
    }
    val host = remember(a) {
        ExponentialHost(
            HostOptions(
                transport = JsonlStreamTransport(
                    url = "$server/a2ui.jsonl" + if (live) "" else "?once=1",
                    postUrl = "$server/action",
                    reconnectMs = if (live) 2000 else 0,
                ),
                extensions = listOf(HostExtension(a.extension, mapOf("TrendLine" to TrendLinePainter))),
                theme = ThemeHandle.load(a.theme),
                mode = mode,
                // Links open in the browser (the URL policy's default schemes).
                policy = HostPolicy(),
            ),
        )
    }
    DisposableEffect(host) {
        host.connect()
        onDispose { host.dispose() }
    }
    val background = host.theme?.color("background", mode) ?: Color.White
    Column(
        Modifier
            .fillMaxSize()
            .background(background)
            .safeDrawingPadding()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
    ) {
        HostSurface(host, "greenhouse", fallback = { Text("Waiting for the surface…") })
        Text(
            "transport: ${host.status.wire}",
            Modifier.padding(top = 8.dp),
            style = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 12.sp, color = (host.theme?.ink(mode) ?: Color.Black).copy(alpha = 0.6f)),
        )
    }
}
