package com.exponential.app.ui.styleguide

import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.VoiceCall
import com.exponential.app.ui.call.VoiceCallSpecimen
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens

// NEW-69 / S-3 (fixtures/voice-call.json): the Android twin of the styleguide
// entry `voice-call` — THE phone-call screen the live call draws, once per
// `surfaceSpecimens` row (ringing, getting ready, in call speaking /
// listening, muted): web-mobile's gallery, the frames stacked full width in
// one column, the row's name captioned under each, static (the rings at
// their rest phase). Capture-only (shot `sg_voice-call`,
// StyleguideScreenshotsTest#captureOnlyScreens).

const val VoiceCallSpecimenTestId = "voice-call-specimen"

/** One frame's height (web-mobile's `h-[640px] w-full`). */
private val FRAME_HEIGHT = 640.dp
private val FRAME_SHAPE = RoundedCornerShape(28.dp)

@Composable
fun VoiceCallSpecimenScreen() {
    AppBackground {
        Box(Modifier.fillMaxSize().statusBarsPadding().testTag(VoiceCallSpecimenTestId)) {
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                for (specimen in VoiceCall.surfaceSpecimens) {
                    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Box(
                            Modifier
                                .fillMaxWidth()
                                .height(FRAME_HEIGHT)
                                .clip(FRAME_SHAPE)
                                .border(GlassTokens.Hairline, GlassTokens.StrokeCard, FRAME_SHAPE),
                        ) {
                            VoiceCallSpecimen(specimen, Modifier.fillMaxSize())
                        }
                        Text(
                            specimen.name,
                            style = MaterialTheme.typography.labelSmall,
                            color = DesignTokens.Palette.MutedForeground,
                            textAlign = TextAlign.Center,
                            modifier = Modifier.fillMaxWidth(),
                            maxLines = 1,
                        )
                    }
                }
            }
        }
    }
}
