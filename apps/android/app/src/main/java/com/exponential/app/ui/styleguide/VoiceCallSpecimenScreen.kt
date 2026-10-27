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
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.VoiceCall
import com.exponential.app.ui.call.VoiceCallSpecimenCard
import com.exponential.app.ui.theme.AppBackground

// NEW-69 (fixtures/voice-call.json): the Android twin of the styleguide entry
// `voice-call` — THE in-call card the live call screen draws, once per
// `surfaceSpecimens` row, static (the wave at its rest phase). Capture-only
// (shot `sg_voice-call`, StyleguideScreenshotsTest#captureOnlyScreens).

const val VoiceCallSpecimenTestId = "voice-call-specimen"

@Composable
fun VoiceCallSpecimenScreen() {
    AppBackground {
        Box(Modifier.fillMaxSize().statusBarsPadding().testTag(VoiceCallSpecimenTestId)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                for (specimen in VoiceCall.surfaceSpecimens) {
                    VoiceCallSpecimenCard(specimen, Modifier.fillMaxWidth())
                }
            }
        }
    }
}
