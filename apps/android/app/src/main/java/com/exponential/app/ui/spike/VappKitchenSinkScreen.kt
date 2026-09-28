package com.exponential.app.ui.spike

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * VAPP-4 spike: the shared kitchen-sink fixture (or the 200-node bench tree)
 * laid out by the Rust taffy core and painted with the app's own primitives.
 */
@Composable
fun VappKitchenSinkScreen(bench: Int?, forceRtl: Boolean) {
    val state = rememberVappSurfaceState(bench, forceRtl)
    val sample by state.sample.collectAsState()
    val direction = if (forceRtl) LayoutDirection.Rtl else LocalLayoutDirection.current
    CompositionLocalProvider(LocalLayoutDirection provides direction) {
        Column(
            Modifier
                .fillMaxSize()
                .background(DesignTokens.Palette.Background)
                .statusBarsPadding()
                .testTag("vapp-kitchen-sink"),
        ) {
            // The timing caption, top-right (top-left under RTL).
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
                horizontalArrangement = Arrangement.End,
            ) {
                val s = sample
                Text(
                    text = if (s == null) {
                        "…"
                    } else {
                        "${s.nodes} nodes · ${s.measureCalls} calls · " +
                            "taffy ${s.taffyNs / 1000} µs · wall ${s.wallNs / 1000} µs"
                    },
                    modifier = Modifier
                        .background(GlassTokens.CardFill, RoundedCornerShape(6.dp))
                        .padding(horizontal = 6.dp, vertical = 2.dp)
                        // Tap = one more layout pass (a fresh timing sample).
                        .clickable { state.relayoutTick++ }
                        .testTag("vapp-caption"),
                    fontSize = 11.sp,
                    lineHeight = 14.sp,
                    fontFamily = FontFamily.Monospace,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            }
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .navigationBarsPadding(),
            ) {
                VappSurface(state, Modifier.fillMaxWidth().testTag("vapp-surface"))
            }
        }
    }
}
