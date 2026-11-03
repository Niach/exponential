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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ExponentialUi.appContext = applicationContext // links open through it
        host.connect()
        setContent {
            // The surface the agent creates, as tall as its content: you own the scroller.
            Column(Modifier.safeDrawingPadding().verticalScroll(rememberScrollState()).padding(16.dp)) {
                HostSurface(host, "main", fallback = { Text("Waiting for the agent…") })
            }
        }
    }

    override fun onDestroy() {
        host.dispose()
        super.onDestroy()
    }
}
