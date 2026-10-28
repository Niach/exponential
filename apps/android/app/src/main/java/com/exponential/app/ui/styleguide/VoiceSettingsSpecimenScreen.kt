package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import com.exponential.app.domain.Voice
import com.exponential.app.ui.components.picker.DevicePickerDevice
import com.exponential.app.ui.settings.VoiceSettingsForm
import com.exponential.app.ui.settings.VoiceSettingsScaffold
import com.exponential.app.ui.settings.VoiceSettingsViewModel
import com.exponential.app.ui.theme.AppBackground

// NEW-71 (fixtures/voice.json): the Android twin of the catalog view
// `settings-voice` — THE Voice settings page (its chrome and form) over the
// seed's rows (seed-screenshots.ts: Alex's MacBook Pro as the voice device,
// Triage as the agent, Speak replies + Push to talk on, German), so the tray
// group shows. Capture-only (shot `sg_settings-voice`,
// StyleguideScreenshotsTest#captureOnlyScreens); a change stays local.

const val VoiceSettingsSpecimenTestId = "settings-voice-specimen"

private const val DEVICE_ID = "screenshot-demo-desktop"
private const val DEVICE_NAME = "Alex's MacBook Pro"
private const val TRIAGE_ID = "specimen-agent-triage"

private val VOCABULARY = run {
    val deviceOptions = Voice.voiceDeviceOptions(
        listOf(Voice.DeviceLike(deviceId = DEVICE_ID, name = DEVICE_NAME, kind = "desktop")),
    )
    VoiceSettingsViewModel.Vocabulary(
        deviceOptions = deviceOptions,
        deviceRows = listOf(DevicePickerDevice(id = DEVICE_ID, name = DEVICE_NAME, icon = null, isServer = false)),
        agentOptions = Voice.agentOptions(
            listOf(
                Voice.AgentLike(id = TRIAGE_ID, name = "Triage", team = "Acme"),
                Voice.AgentLike(id = "specimen-agent-designer", name = "Designer", team = "Acme"),
            ),
        ),
        deviceNames = mapOf(DEVICE_ID to DEVICE_NAME),
        agentNames = mapOf(TRIAGE_ID to "Triage", "specimen-agent-designer" to "Designer"),
    )
}

private val SETTINGS = Voice.Settings(
    voiceDeviceId = DEVICE_ID,
    agentId = TRIAGE_ID,
    speakReplies = true,
    pushToTalk = true,
    pttShortcut = "ctrl+space",
    wakeWord = false,
    wakePhrase = "",
    language = "de",
)

@Composable
fun VoiceSettingsSpecimenScreen() {
    var settings by remember { mutableStateOf(SETTINGS) }
    var wakePhrase by remember { mutableStateOf("") }
    var voiceFile by remember { mutableStateOf("") }
    var voiceTranscript by remember { mutableStateOf("") }
    AppBackground {
        Box(Modifier.fillMaxSize().testTag(VoiceSettingsSpecimenTestId)) {
            VoiceSettingsScaffold(onBack = {}) { padding ->
                VoiceSettingsForm(
                    settings = settings,
                    vocabulary = VOCABULARY,
                    wakePhrase = wakePhrase,
                    wakePhraseError = null,
                    onChange = { settings = Voice.clearOnNone(settings, it) },
                    onWakePhraseChange = { wakePhrase = it },
                    onWakePhraseDone = {},
                    voiceFile = voiceFile,
                    onVoiceFileChange = { voiceFile = it },
                    onVoiceFileDone = {},
                    voiceTranscript = voiceTranscript,
                    onVoiceTranscriptChange = { voiceTranscript = it },
                    onVoiceTranscriptDone = {},
                    modifier = Modifier.padding(padding),
                )
            }
        }
    }
}
