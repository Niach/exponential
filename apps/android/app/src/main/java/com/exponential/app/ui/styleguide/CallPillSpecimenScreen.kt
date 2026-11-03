package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.VoiceCall
import com.exponential.app.ui.call.VoiceCallPill
import com.exponential.app.ui.theme.AppBackground

// EXP-1681 (voice-call.json `pill`): the Android twin of the styleguide
// entry `call-pill`: THE floating call pill a minimized call draws, once per
// `surfaceSpecimens` row, static. Capture-only (shot `sg_call-pill`,
// StyleguideScreenshotsTest#captureOnlyScreens).

const val CallPillSpecimenTestId = "call-pill-specimen"

@Composable
fun CallPillSpecimenScreen() {
    AppBackground {
        BoxWithConstraints(Modifier.fillMaxSize().statusBarsPadding().testTag(CallPillSpecimenTestId)) {
            // `pill.width`, inside the 16 dp page margins.
            val width = minOf(VoiceCall.Pill.WIDTH.dp, maxWidth - 32.dp)
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                for (specimen in VoiceCall.surfaceSpecimens) {
                    VoiceCallPill(
                        agent = specimen.agent,
                        view = specimen.view,
                        timer = specimen.timer,
                        level = specimen.level.toFloat(),
                        bands = specimen.bands,
                        onMute = {},
                        onEnd = {},
                        width = width,
                        still = true,
                    )
                }
            }
        }
    }
}
