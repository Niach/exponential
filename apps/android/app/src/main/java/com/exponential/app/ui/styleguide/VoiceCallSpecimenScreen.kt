package com.exponential.app.ui.styleguide

import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.VoiceCall
import com.exponential.app.ui.call.VoiceCallSpecimen
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens

// NEW-69 / S-3 (fixtures/voice-call.json): the Android twin of the styleguide
// entry `voice-call` — THE phone-call screen the live call draws, once per
// `surfaceSpecimens` row (ringing, getting ready, in call speaking /
// listening, muted), each a phone frame scaled to fit three a row, static
// (the rings at their rest phase). Capture-only (shot `sg_voice-call`,
// StyleguideScreenshotsTest#captureOnlyScreens).

const val VoiceCallSpecimenTestId = "voice-call-specimen"

/** One frame's phone size before it is scaled down. */
private val FRAME_WIDTH = 360.dp
private val FRAME_HEIGHT = 720.dp

@Composable
fun VoiceCallSpecimenScreen() {
    AppBackground {
        Box(Modifier.fillMaxSize().statusBarsPadding().testTag(VoiceCallSpecimenTestId)) {
            BoxWithConstraints(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(12.dp),
            ) {
                val gap = 8.dp
                val cell = (maxWidth - gap * 2) / 3
                val scale = cell / FRAME_WIDTH
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    for (row in VoiceCall.surfaceSpecimens.chunked(3)) {
                        Row(horizontalArrangement = Arrangement.spacedBy(gap), verticalAlignment = Alignment.Top) {
                            for (specimen in row) {
                                Column(Modifier.width(cell), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                    Box(
                                        Modifier
                                            .size(cell, FRAME_HEIGHT * scale)
                                            .clip(RoundedCornerShape(16.dp))
                                            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, RoundedCornerShape(16.dp)),
                                        contentAlignment = Alignment.Center,
                                    ) {
                                        VoiceCallSpecimen(
                                            specimen,
                                            Modifier
                                                .requiredSize(FRAME_WIDTH, FRAME_HEIGHT)
                                                .graphicsLayer {
                                                    scaleX = scale
                                                    scaleY = scale
                                                },
                                        )
                                    }
                                    Text(
                                        specimen.name,
                                        style = MaterialTheme.typography.labelSmall,
                                        color = DesignTokens.Palette.MutedForeground,
                                        modifier = Modifier.fillMaxWidth().padding(horizontal = 2.dp),
                                        maxLines = 1,
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
