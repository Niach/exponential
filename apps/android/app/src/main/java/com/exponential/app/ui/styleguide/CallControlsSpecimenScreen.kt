package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.VoiceCall
import com.exponential.app.ui.components.CallButtonCompactSize
import com.exponential.app.ui.components.CallControls
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.DesignTokens

// S-3 (fixtures/voice-call.json PHONE-CALL UI): the Android twin of the
// styleguide entry `call-controls` — THE round icon-only call buttons
// ([CallControls]): ringing (only the red end, `Cancel` under it), in call
// (mute, speaker, end), muted with the earpiece (both toggled on the
// inverted fill), and the compact row. Capture-only (shot
// `sg_call-controls`, StyleguideScreenshotsTest#captureOnlyScreens).

const val CallControlsSpecimenTestId = "call-controls-specimen"

private val PHONE = listOf(VoiceCall.Button.Mute, VoiceCall.Button.Speaker, VoiceCall.Button.End)

private val ROWS: List<Pair<String, VoiceCall.View>> = listOf(
    "Ringing" to VoiceCall.View(VoiceCall.Activity.Ringing, VoiceCall.Copy.CALLING, "", false, true, listOf(VoiceCall.Button.End), VoiceCall.Copy.CANCEL),
    "In call" to VoiceCall.View(VoiceCall.Activity.Listening, null, "", false, true, PHONE, VoiceCall.Copy.END_CALL),
    "Muted, speaker off" to VoiceCall.View(VoiceCall.Activity.Muted, null, VoiceCall.Copy.MUTED, true, false, PHONE, VoiceCall.Copy.END_CALL),
)

@Composable
fun CallControlsSpecimenScreen() {
    AppBackground {
        Box(Modifier.fillMaxSize().statusBarsPadding().testTag(CallControlsSpecimenTestId)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(28.dp),
            ) {
                for ((name, view) in ROWS) {
                    SpecimenRow(name) { CallControls(view = view, onMute = {}, onSpeaker = {}, onEnd = {}) }
                }
                SpecimenRow("Compact") {
                    CallControls(view = ROWS[1].second.copy(buttons = listOf(VoiceCall.Button.Mute, VoiceCall.Button.End)), onMute = {}, onSpeaker = {}, onEnd = {}, size = CallButtonCompactSize)
                }
            }
        }
    }
}

@Composable
private fun SpecimenRow(name: String, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(name, style = MaterialTheme.typography.labelMedium, color = DesignTokens.Palette.MutedForeground)
        content()
    }
}
