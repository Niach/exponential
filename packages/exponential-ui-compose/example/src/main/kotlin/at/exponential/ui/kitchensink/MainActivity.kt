package at.exponential.ui.kitchensink

import android.graphics.Color as AndroidColor
import android.os.Bundle
import android.util.Log
import android.view.View
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.theme.Mode
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import java.io.File

/**
 * The Exponential UI kitchen sink as a BLANK Android app: it links
 * `:ui-compose` and nothing from the Exponential app, and renders
 * `packages/exponential-ui/fixtures/kitchen-sink.json` (copied into the
 * assets at build time). What `shots/exponential-ui-kitchen-sink/android.webp`
 * is captured from (`--es shot exponential-ui-kitchen-sink`). The launch
 * extras are [LaunchOptions]'.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val options = LaunchOptions.parse(intent)
        setContent {
            val scope = rememberCoroutineScope()
            val state = remember { SinkState(this, options, scope) }
            SystemBars(state)
            SinkScreen(state, rootView = window.decorView)
        }
    }

    /** Edge to edge, the bars' icons following the picked mode over the theme's background. */
    @Composable
    private fun SystemBars(state: SinkState) {
        val dark = state.mode == Mode.Dark
        LaunchedEffect(dark) {
            val transparent = AndroidColor.TRANSPARENT
            val style = if (dark) SystemBarStyle.dark(transparent) else SystemBarStyle.light(transparent, transparent)
            enableEdgeToEdge(statusBarStyle = style, navigationBarStyle = style)
        }
    }
}

/** The screen: the chrome row (unless `shot`), then the scrolling surface and the host line. */
@Composable
fun SinkScreen(state: SinkState, rootView: View) {
    val options = state.options
    val model = state.model
    val dark = state.mode == Mode.Dark
    val background = model.color("background") ?: if (dark) Color.Black else Color.White
    val muted = model.color("mutedForeground") ?: Color.Gray
    val primary = model.color("primary")
    val scheme = (if (dark) darkColorScheme() else lightColorScheme()).let { s ->
        s.copy(
            primary = primary ?: s.primary,
            onPrimary = model.color("primaryForeground") ?: s.onPrimary,
            background = background,
            surface = model.color("popover") ?: background,
            onSurface = model.color("foreground") ?: s.onSurface,
            onBackground = model.color("foreground") ?: s.onBackground,
        )
    }
    val scope = rememberCoroutineScope()
    val dump = remember { if (options.dump != null) SurfaceDump() else null }
    val density = LocalDensity.current.density

    LaunchedEffect(model) {
        snapshotFlow { model.passCount }.first { it > 0 }
        Bench.logCold(model)
        delay(2000)
        options.a11yDump?.let { path ->
            val labels = AccessibilityWalk.labels(rootView)
            writeText(rootView, path, labels.joinToString("\n") + "\n---\n" + AccessibilityWalk.trace(rootView).joinToString("\n") + "\n")
        }
        options.dump?.let { path -> dump?.write(resolve(rootView, path)) }
        if (options.benchLoop) Bench.run(model)
    }

    MaterialTheme(colorScheme = scheme) {
        Column(
            Modifier
                .fillMaxSize()
                .background(background)
                .windowInsetsPadding(WindowInsets.safeDrawing),
        ) {
            if (options.shot == null) ChromeRow(state)
            Column(
                Modifier
                    .fillMaxSize()
                    // The visible height reaches the model BEFORE the surface
                    // measures, so the first pass already knows it (one cold
                    // pass, not two).
                    .layout { measurable, constraints ->
                        if (constraints.hasBoundedHeight) {
                            val h = constraints.maxHeight / density
                            if (h != model.viewportHeight) model.setViewport(model.width, h)
                        }
                        val placeable = measurable.measure(constraints)
                        layout(placeable.width, placeable.height) { placeable.place(0, 0) }
                    }
                    .verticalScroll(rememberScrollState()),
            ) {
                Box((dump?.modifier ?: Modifier).background(background)) {
                    ExponentialSurface(model, options.width?.let { Modifier.width(it.dp) } ?: Modifier)
                }
                Text(
                    "host: ${state.echo}",
                    style = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 12.sp, color = muted),
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { scope.launch { Bench.run(model) } }
                        .padding(horizontal = 16.dp, vertical = 8.dp)
                        .testTag("host-echo"),
                )
            }
        }
    }
}

/** The theme dropdown (built-ins + `brand`) and the Dark / Light toggle. */
@Composable
private fun ChromeRow(state: SinkState) {
    var open by remember { mutableStateOf(false) }
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box {
            TextButton(onClick = { open = true }) { Text("Theme: ${state.themeId}") }
            DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
                for (id in SinkState.themeIds) {
                    DropdownMenuItem(text = { Text(id) }, onClick = {
                        open = false
                        state.pickTheme(id)
                    })
                }
            }
        }
        Spacer(Modifier.weight(1f))
        SingleChoiceSegmentedButtonRow(Modifier.width(180.dp)) {
            val modes = listOf(Mode.Dark to "Dark", Mode.Light to "Light")
            modes.forEachIndexed { i, (m, label) ->
                SegmentedButton(
                    selected = state.mode == m,
                    onClick = { state.pickMode(m) },
                    shape = SegmentedButtonDefaults.itemShape(i, modes.size),
                ) { Text(label) }
            }
        }
    }
}

/**
 * Where a dump lands: the path as given when the app may write it, else
 * the same file name in the app's external files dir
 * (`/sdcard/Android/data/at.exponential.ui.kitchensink/files/`, adb-pullable).
 */
private fun resolve(view: View, path: String): String {
    val f = File(path)
    val writable = runCatching { f.parentFile?.mkdirs(); f.parentFile?.canWrite() == true }.getOrDefault(false)
    if (f.isAbsolute && writable) return path
    val dir = view.context.getExternalFilesDir(null) ?: view.context.filesDir
    return File(dir, f.name).absolutePath
}

private fun writeText(view: View, path: String, text: String) {
    val target = resolve(view, path)
    runCatching { File(target).writeText(text) }
        .onSuccess { Log.i(LOG_TAG, "a11yDump → $target") }
        .onFailure { Log.w(LOG_TAG, "a11yDump $target: ${it.message}") }
}
