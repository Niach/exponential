package at.exponential.guide

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import at.exponential.ui.ExponentialUi
import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.HostSurface
import at.exponential.ui.host.JsonlStreamTransport
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle

class MainActivity : ComponentActivity() {
    // One host per app: A2UI messages in over a transport, actions back out.
    private val host = ExponentialHost(
        HostOptions(
            transport = JsonlStreamTransport(url = "http://10.0.2.2:4300/a2ui.jsonl", postUrl = "http://10.0.2.2:4300/action"),
            theme = ThemeHandle.builtin("exponential"),
            mode = Mode.Dark,
        ),
    )

    // The Express stream (`--ez express true` or the button): a host of its own.
    private var express by mutableStateOf<ExponentialHost?>(null)

    private fun openExpress() {
        express = expressHost().also { it.connect() }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ExponentialUi.appContext = applicationContext // links open through it
        if (intent.getBooleanExtra("express", false)) openExpress() else host.connect()
        setContent {
            val stream = express
            if (stream != null) {
                ExpressStream(stream)
            } else {
                // The surface the agent creates, as tall as its content: you own the scroller.
                Column(Modifier.safeDrawingPadding().verticalScroll(rememberScrollState()).padding(16.dp)) {
                    HostSurface(host, "main", fallback = { Text("Waiting for the agent…") })
                    TextButton(onClick = ::openExpress) { Text("Express stream") }
                }
            }
        }
    }

    override fun onDestroy() {
        express?.dispose()
        host.dispose()
        super.onDestroy()
    }
}
