package com.exponential.app.ui.styleguide

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.R
import com.exponential.app.data.push.LiveActivity
import com.exponential.app.data.push.NotificationActions
import androidx.compose.material3.Text

// #EXP-1639 / #EXP-1641: the Android twins of the iOS capture specimens
// `os-live-activity`, `os-live-activity-island` and `os-notification-actions`.
// The OS draws the real ones (a promoted `Notification.ProgressStyle` Live
// Update and its status-bar chip; `NotificationCompat.Action`s + RemoteInput)
// and a test cannot expand a RemoteViews notification, so these are faithful
// MOCKS of the shade, fed by the REAL models: [LiveActivity.content] over
// live-activity.json's `contentCases` and [NotificationActions.notificationActions]
// over notification-actions.json's cases. Capture-only
// (StyleguideScreenshotsTest#captureOnlyScreens), not Exponential UI primitives.

const val OsLiveActivitySpecimenTestId = "os-live-activity-specimen"
const val OsLiveActivityChipSpecimenTestId = "os-live-activity-chip-specimen"
const val OsNotificationActionsSpecimenTestId = "os-notification-actions-specimen"

private val ShadeBackground = Color(0xFF1B1B1F)
private val CardBackground = Color(0xFF2B2B30)
private val FieldBackground = Color(0xFF3A3A40)
private val Accent = Color(0xFFA8C7FA)
private val OnShade = Color(0xFFE4E4E7)
private val OnShadeMuted = Color(0xFFA1A1AA)
private const val TIMER = "12:34"

private fun session(
    title: String = "Reduce cold start",
    sealedTitle: Boolean = false,
    status: String = "running",
    needsInput: Boolean = false,
    blocked: Boolean = false,
) = LiveActivity.Session(
    id = "s1", parentId = null, userId = "u1", agentId = null, startedByAgentId = null,
    title = title, sealedTitle = sealedTitle, status = status, busy = status == "running",
    needsInput = needsInput, blocked = blocked, startedAt = "2026-11-06T08:00:00.000Z",
)

/** live-activity.json `contentCases`, in order, through the real model. */
private val liveCases: List<LiveActivity.Content> = listOf(
    LiveActivity.content(session(), "Mira's MacBook", null, null, null),
    LiveActivity.content(
        session(status = "idle", needsInput = true), null, null,
        "Question: Which database should we use for the cache?", null,
    ),
    LiveActivity.content(session(sealedTitle = true), "Mira's MacBook", null, null, null),
    LiveActivity.content(session(), null, 2041, null, null),
    LiveActivity.content(session(blocked = true), null, null, null, "Rate limited"),
    LiveActivity.content(session(status = "idle"), null, 2041, null, null),
)

@Composable
private fun Shade(testId: String, content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize().background(ShadeBackground).statusBarsPadding().testTag(testId)) {
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) { content() }
    }
}

@Composable
private fun Caption(text: String) {
    Text(
        text,
        color = OnShadeMuted,
        fontSize = 13.sp,
        fontWeight = FontWeight.SemiBold,
        modifier = Modifier.padding(start = 4.dp, top = 4.dp),
    )
}

/** The small icon as the shade tints it: white glyph on the accent disc. */
@Composable
private fun SmallIcon(size: Int = 24) {
    Box(Modifier.size(size.dp).clip(CircleShape).background(Accent), contentAlignment = Alignment.Center) {
        Image(
            painterResource(R.drawable.ic_stat_notification),
            contentDescription = null,
            colorFilter = ColorFilter.tint(ShadeBackground),
            modifier = Modifier.size((size * 0.7f).dp),
        )
    }
}

/** The header line: app name · the chip text, else the chronometer, else `now`. */
@Composable
private fun Header(trailing: String) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text("Exponential", color = OnShadeMuted, fontSize = 12.sp)
        Text("·", color = OnShadeMuted, fontSize = 12.sp)
        Text(trailing, color = OnShadeMuted, fontSize = 12.sp)
    }
}

/** `Notification.ProgressStyle`, styled by progress: four segments, filled through [segment]. */
@Composable
private fun Segments(segment: Int) {
    Row(Modifier.fillMaxWidth().height(6.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        LiveActivity.SEGMENTS.indices.forEach { index ->
            Box(
                Modifier.weight(1f).height(6.dp).clip(RoundedCornerShape(3.dp))
                    .background(if (index < segment) Accent else Accent.copy(alpha = 0.25f)),
            )
        }
    }
}

@Composable
private fun LiveUpdate(content: LiveActivity.Content) {
    val chip = LiveActivity.chipText(content.badge)
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp)).background(CardBackground)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            SmallIcon(32)
            Column(Modifier.weight(1f)) {
                Header(chip ?: TIMER)
                Text(content.title, color = OnShade, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                Text(content.subtitle, color = OnShade, fontSize = 14.sp)
                content.detail?.let { Text(it, color = OnShadeMuted, fontSize = 14.sp, lineHeight = 18.sp, maxLines = 2, overflow = TextOverflow.Ellipsis) }
            }
        }
        Segments(content.segment)
    }
}

/** The Live Update in the shade, once per `contentCases` row. */
@Composable
fun OsLiveActivitySpecimenScreen() {
    Shade(OsLiveActivitySpecimenTestId) {
        Caption("Live Update")
        liveCases.forEach { LiveUpdate(it) }
    }
}

/** A status bar with the promoted Live Update's chip beside the clock. */
@Composable
private fun StatusBar(content: LiveActivity.Content) {
    val chip = LiveActivity.chipText(content.badge) ?: TIMER
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Color.Black).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("9:41", color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.Medium)
        Spacer(Modifier.width(10.dp))
        Row(
            Modifier.clip(CircleShape).background(Accent).padding(start = 4.dp, end = 10.dp, top = 3.dp, bottom = 3.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Image(
                painterResource(R.drawable.ic_stat_notification),
                contentDescription = null,
                colorFilter = ColorFilter.tint(ShadeBackground),
                modifier = Modifier.size(16.dp),
            )
            Text(chip, color = ShadeBackground, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
        }
        Spacer(Modifier.weight(1f))
        Row(horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(width = 14.dp, height = 10.dp).clip(RoundedCornerShape(2.dp)).background(Color.White))
            Box(Modifier.size(width = 22.dp, height = 11.dp).border(1.5.dp, Color.White, RoundedCornerShape(3.dp)).padding(2.dp)) {
                Box(Modifier.fillMaxSize().clip(RoundedCornerShape(1.dp)).background(Color.White))
            }
        }
    }
}

/** The collapsed promoted notification at the top of the shade. */
@Composable
private fun Collapsed(content: LiveActivity.Content) {
    val chip = LiveActivity.chipText(content.badge) ?: TIMER
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp)).background(CardBackground).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        SmallIcon(28)
        Column(Modifier.weight(1f)) {
            Text(content.title, color = OnShade, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
            Text(content.subtitle, color = OnShadeMuted, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Text(chip, color = Accent, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
    }
}

/** iOS's Dynamic Island counterpart: the status-bar chip + the collapsed Live Update. */
@Composable
fun OsLiveActivityChipSpecimenScreen() {
    Shade(OsLiveActivityChipSpecimenTestId) {
        Caption("Status bar chip")
        StatusBar(liveCases[0])
        StatusBar(liveCases[1])
        StatusBar(liveCases[3])
        Caption("Collapsed")
        Collapsed(liveCases[1])
        Collapsed(liveCases[0])
        Collapsed(liveCases[4])
    }
}

private class Push(val title: String, val body: String, val data: Map<String, String>)

/** notification-actions.json's three kinds of push, as iOS's specimen shows them. */
private val pushes = listOf(
    Push(
        "Reduce cold start",
        "Question: Which database should we use for the cache?",
        mapOf("sessionId" to "s", "messageId" to "m", "cardKind" to "question", "options" to "[\"Postgres\",\"SQLite\"]", "custom" to "1"),
    ),
    Push(
        "Reduce cold start",
        "Plan: Cache the parsed config and lazy-load the editor",
        mapOf("sessionId" to "s", "messageId" to "m", "cardKind" to "plan", "planQuestions" to "0"),
    ),
    Push("Release notes", "Mira: @you can you check the wording?", mapOf("sessionId" to "s")),
)

@Composable
private fun Notification(push: Push, reply: String? = null) {
    val actions = NotificationActions.notificationActions(push.data, keyHeld = false, openedAskText = null)
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp)).background(CardBackground)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            SmallIcon(28)
            Column(Modifier.weight(1f)) {
                Header("now")
                Text(push.title, color = OnShade, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                Text(push.body, color = OnShade, fontSize = 14.sp, lineHeight = 18.sp, maxLines = 3, overflow = TextOverflow.Ellipsis)
            }
        }
        if (reply != null) {
            // RemoteInput open: the inline field with its send button.
            Row(
                Modifier.fillMaxWidth().clip(CircleShape).background(FieldBackground).padding(start = 16.dp, end = 6.dp, top = 6.dp, bottom = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(reply, color = OnShadeMuted, fontSize = 15.sp, modifier = Modifier.weight(1f))
                Box(
                    Modifier.clip(CircleShape).background(Accent).padding(horizontal = 14.dp, vertical = 8.dp),
                ) {
                    Text(NotificationActions.SEND_LABEL, color = ShadeBackground, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
                }
            }
        } else {
            Row(Modifier.padding(start = 40.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                actions.forEach { action ->
                    Box(
                        Modifier.clip(CircleShape).background(FieldBackground).padding(horizontal = 12.dp, vertical = 8.dp),
                    ) {
                        Text(action.title, color = Accent, fontSize = 14.sp, fontWeight = FontWeight.Medium, maxLines = 1)
                    }
                }
            }
        }
    }
}

/** The three pushes with their actions, then the chat's Reply open (RemoteInput). */
@Composable
fun OsNotificationActionsSpecimenScreen() {
    Shade(OsNotificationActionsSpecimenTestId) {
        Caption("Question")
        Notification(pushes[0])
        Caption("Plan")
        Notification(pushes[1])
        Caption("Chat")
        Notification(pushes[2])
        Caption("Reply")
        Notification(pushes[2], reply = NotificationActions.REPLY_PLACEHOLDER)
    }
}
