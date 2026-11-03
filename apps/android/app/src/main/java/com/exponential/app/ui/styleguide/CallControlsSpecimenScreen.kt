package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
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
import com.exponential.app.ui.call.VoiceCallPill
import com.exponential.app.ui.components.CallButton
import com.exponential.app.ui.components.CallButtonTone
import com.exponential.app.ui.components.CallControls
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.DesignTokens

// S-3 (fixtures/voice-call.json PHONE-CALL UI): the Android twin of the
// styleguide entry `call-controls` — web's gallery (`CallControlsSpecimen`)
// with its captions: THE round call buttons in every tone (neutral, pressed,
// speaker, speaker off, destructive, ringing with Cancel under the red end),
// then the rows (phones in call; web + desktop in call, muted; ringing), then
// the compact row (EXP-1681: the floating call pill, `call-pill`). Capture-only (shot `sg_call-controls`,
// StyleguideScreenshotsTest#captureOnlyScreens).

const val CallControlsSpecimenTestId = "call-controls-specimen"

private val C = VoiceCall.Copy

private fun view(activity: VoiceCall.Activity, muted: Boolean, buttons: List<VoiceCall.Button>, end: String) =
    VoiceCall.View(activity, null, "", muted, true, buttons, end)

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun CallControlsSpecimenScreen() {
    AppBackground {
        Box(Modifier.fillMaxSize().statusBarsPadding().testTag(CallControlsSpecimenTestId)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(32.dp),
            ) {
                FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(32.dp), verticalArrangement = Arrangement.spacedBy(32.dp)) {
                    Labelled("neutral") { CallButton(ExpIcons.callMute, C.MUTE, onClick = {}) }
                    Labelled("pressed") { CallButton(ExpIcons.callUnmute, C.UNMUTE, onClick = {}, tone = CallButtonTone.Toggled) }
                    Labelled("speaker") { CallButton(ExpIcons.callSpeaker, C.SPEAKER, onClick = {}) }
                    Labelled("speaker off") { CallButton(ExpIcons.callSpeakerOff, C.SPEAKER_OFF, onClick = {}) }
                    Labelled("destructive") { CallButton(ExpIcons.callEnd, C.END_CALL, onClick = {}, tone = CallButtonTone.End) }
                    Labelled("ringing: label under") { CallButton(ExpIcons.callEnd, C.CANCEL, onClick = {}, tone = CallButtonTone.End, showLabel = true) }
                }
                FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(48.dp), verticalArrangement = Arrangement.spacedBy(32.dp)) {
                    Labelled("phones in call") {
                        CallControls(view(VoiceCall.Activity.Listening, false, listOf(VoiceCall.Button.Mute, VoiceCall.Button.Speaker, VoiceCall.Button.End), C.END_CALL), onMute = {}, onSpeaker = {}, onEnd = {})
                    }
                    Labelled("web + desktop in call, muted") {
                        CallControls(view(VoiceCall.Activity.Muted, true, listOf(VoiceCall.Button.Mute, VoiceCall.Button.End), C.END_CALL), onMute = {}, onSpeaker = {}, onEnd = {})
                    }
                    Labelled("ringing") {
                        CallControls(view(VoiceCall.Activity.Ringing, false, listOf(VoiceCall.Button.End), C.CANCEL), onMute = {}, onSpeaker = {}, onEnd = {})
                    }
                }
                val inCall = VoiceCall.surfaceSpecimens.first { it.name == "in call, speaking" }
                Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.TopCenter) {
                    Labelled("compact row: the floating call pill") {
                        VoiceCallPill(
                            agent = inCall.agent,
                            view = inCall.view,
                            timer = inCall.timer,
                            level = inCall.level.toFloat(),
                            bands = inCall.bands,
                            onMute = {},
                            onEnd = {},
                            still = true,
                        )
                    }
                }
            }
        }
    }
}

/** Web `Labelled`: the control in an 80 dp tall slot over its caption, centred. */
@Composable
private fun Labelled(label: String, content: @Composable () -> Unit) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.heightIn(min = 80.dp), contentAlignment = Alignment.TopCenter) { content() }
        Text(label, style = MaterialTheme.typography.labelSmall, color = DesignTokens.Palette.MutedForeground)
    }
}
