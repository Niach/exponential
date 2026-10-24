package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.markdown.LocalMentions
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.MentionResolver
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.thread.ThreadConversation
import com.exponential.app.ui.thread.ThreadRowMeta

// Thread e4a04d22: the Android twin of the styleguide entry `mention-pill`
// (shot `sg_mention-pill`) — the comments' round member pill wherever a chat
// names a member: a known `@email` is the member's name pill, an unknown
// address stays plain text. Capture-only (no server, no sign-in), drawn by
// the REAL MarkdownView + ThreadConversation under a LocalMentions resolver.

const val MentionPillSpecimenTestId = "mention-pill-specimen"

private val MEMBERS = listOf(MentionMember("Dana Reyes", "dana@acme.dev"))

private fun mentionMessage(id: String, author: String, body: String) =
    SessionTree.ThreadRow.MessageRow(
        id = id,
        author = author,
        authorId = if (author == "user") "u-dana" else null,
        body = body,
        queued = false,
    )

private val MENTION_TRANSCRIPT: List<SessionTree.ThreadRow> = listOf(
    mentionMessage("m1", "user", "Loop in @dana@acme.dev on the iPad sign-in, and cc @x@y.com."),
    mentionMessage("m2", "agent", "Asked @dana@acme.dev to review #APP-19; @x@y.com is not on the team."),
)

private val MENTION_TIMES = mapOf("m1" to "09:02", "m2" to "09:03")

@Composable
fun MentionPillSpecimenScreen() {
    val resolver = remember { MentionResolver(MEMBERS) }
    AppBackground {
        CompositionLocalProvider(LocalMentions provides resolver) {
            Box(Modifier.fillMaxSize().statusBarsPadding().testTag(MentionPillSpecimenTestId)) {
                Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    MentionCaption("Known member · unknown address")
                    MarkdownView(
                        "@dana@acme.dev beside @x@y.com",
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp),
                    )
                    MentionCaption("In a chat")
                    ThreadConversation(
                        rows = MENTION_TRANSCRIPT,
                        rootId = "r",
                        meta = { row ->
                            ThreadRowMeta(authorName = "Dana Reyes", time = MENTION_TIMES[row.id].orEmpty())
                        },
                        onOpenCard = {},
                        onAnswer = { _, _, _ -> },
                        onDecidePlan = { _, _, _ -> },
                        canUnqueue = { false },
                        onUnqueue = {},
                        modifier = Modifier.fillMaxWidth().weight(1f),
                        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 12.dp),
                        onSurfaceReply = {},
                    )
                }
            }
        }
    }
}

@Composable
private fun MentionCaption(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
    )
}
