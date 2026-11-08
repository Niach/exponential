package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.markdown.LocalMarkdownBodyStyle
import com.exponential.app.ui.markdown.LocalMentions
import com.exponential.app.ui.session.TranscriptBodyStyle
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.MentionResolver
import com.exponential.app.ui.theme.AppBackground

// Session e4a04d22: the Android twin of the styleguide entry `mention-pill`
// (shot `sg_mention-pill`). EXP-1553: the web entry's island line for line
// on the bare page — "Ready for review, @Ada Lovelace" (a known `@email` =
// the member's pill) and "cc @x@y.com" (an unknown address stays plain
// text) — drawn by the REAL MarkdownView under a LocalMentions resolver.

const val MentionPillSpecimenTestId = "mention-pill-specimen"

private val MEMBERS = listOf(MentionMember("Ada Lovelace", "ada@example.com"))

@Composable
fun MentionPillSpecimenScreen() {
    val resolver = remember { MentionResolver(MEMBERS) }
    AppBackground {
        // Web's `text-sm`: the chat measure (14sp), not the 17sp document body.
        CompositionLocalProvider(LocalMentions provides resolver, LocalMarkdownBodyStyle provides TranscriptBodyStyle) {
            Box(Modifier.fillMaxSize().statusBarsPadding().testTag(MentionPillSpecimenTestId)) {
                Column(
                    Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 24.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    MarkdownView("Ready for review, @ada@example.com", modifier = Modifier.fillMaxWidth())
                    MarkdownView("cc @x@y.com", modifier = Modifier.fillMaxWidth())
                }
            }
        }
    }
}
