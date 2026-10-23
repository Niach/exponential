package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.Chats
import com.exponential.app.domain.CardSurface
import com.exponential.app.domain.ScheduleDraft
import com.exponential.app.domain.Schedule
import com.exponential.app.domain.SchedulePicker
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.BottomNavBar
import com.exponential.app.ui.components.ChatLeadDisc
import com.exponential.app.ui.components.ScheduleFieldsSpecimen
import com.exponential.app.ui.components.agentMenuItems
import com.exponential.app.ui.components.chatMenuItems
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.thread.ChatHeader
import com.exponential.app.ui.thread.ThreadConversation
import com.exponential.app.ui.thread.ThreadRowMeta
import com.exponential.app.ui.threads.ChatsContent
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

// Thread 58cb32df (fixtures/chats.json): the Android twins of the styleguide
// entries `chat-list`, `phone-tab-bar`, `chat-transcript`, `chat-menus` and
// the updated `schedule-picker` — capture-only screens (no server, no
// sign-in) drawn by the REAL composables over specimen data, shot by
// StyleguideScreenshotsTest#captureOnlyScreens.

const val ChatListSpecimenTestId = "chat-list-specimen"
const val PhoneTabBarSpecimenTestId = "phone-tab-bar-specimen"
const val ChatTranscriptSpecimenTestId = "chat-transcript-specimen"
const val ChatMenusSpecimenTestId = "chat-menus-specimen"
const val SchedulePickerSpecimenTestId = "schedule-picker-specimen"

private val ACME = TeamEntity(id = "t1", name = "Acme", slug = "acme", createdAt = "", updatedAt = "")

private fun chatRow(
    id: String,
    title: String,
    group: SessionTree.ThreadsGroupKey,
    lead: Chats.ChatLead,
    previewKind: String,
    preview: String,
    time: String,
    unread: Int = 0,
    needsYou: Boolean = false,
    busy: Boolean = false,
    word: SessionTree.ThreadRowWord? = null,
    activityAt: String = "",
    deviceId: String? = null,
) = Chats.ChatRow(
    sessionId = id,
    teamId = "t1",
    title = title,
    deviceId = deviceId,
    group = group,
    lead = lead,
    preview = Chats.ChatPreview(previewKind, preview),
    activityAt = activityAt,
    time = time,
    unreadCount = unread,
    needsYou = needsYou,
    busy = busy,
    word = word,
)

/**
 * The canonical specimen data = THE FIXTURE (chats.json): `listCases[0]
 * .expected` (four rows, counts 1/1/1) and `bubbleCases[0].expected` (Triage
 * amber, Designer white, Release none) — what the web entry and the desktop
 * twin render. The app has no fixture at runtime, so the rows are spelled
 * here and `ChatSpecimenFixtureTest` proves them equal to the fixture's.
 */
object ChatListSpecimen {
    val LIST: Chats.ChatList = Chats.ChatList(
        rows = listOf(
            chatRow(
                "b", "Running one", SessionTree.ThreadsGroupKey.Running, Chats.ChatLead.Working,
                "working", Chats.Copy.PREVIEW_WORKING, "09:25", busy = true,
                activityAt = "2026-10-23T09:25:00.000Z",
            ),
            chatRow(
                "a", "Needs you one", SessionTree.ThreadsGroupKey.NeedsYou, Chats.ChatLead.NeedsYou,
                "question", "Question: Snapshot?", "09:10", needsYou = true, busy = true,
                activityAt = "2026-10-23T09:10:00.000Z", deviceId = "dev-mac",
            ),
            chatRow(
                "c", "Open PR", SessionTree.ThreadsGroupKey.Recent, Chats.ChatLead.Open,
                "agent", "Waiting for review", "08:30", unread = 1, word = SessionTree.ThreadRowWord.ReadyToMerge,
                activityAt = "2026-10-23T08:30:00.000Z",
            ),
            chatRow(
                "d", "Merged", SessionTree.ThreadsGroupKey.Recent, Chats.ChatLead.Merged,
                "you", "You: ship it", Chats.Copy.TIME_YESTERDAY,
                activityAt = "2026-10-22T10:00:00.000Z",
            ),
        ),
        counts = Chats.ChatCounts(needsYou = 1, running = 1, unread = 1),
    )

    val BUBBLES: List<Chats.AgentBubble> = listOf(
        Chats.AgentBubble(SessionTree.AgentRow("ag1", "t1", "Triage", "bug", "#a855f7", busy = false, needsYou = true), Chats.AgentRing.NeedsYou),
        Chats.AgentBubble(SessionTree.AgentRow("ag2", "t1", "Designer", "bot", "#6366f1", busy = false, needsYou = false), Chats.AgentRing.Unread),
        Chats.AgentBubble(SessionTree.AgentRow("ag3", "t1", "Release", "package", "#22c55e", busy = false, needsYou = false), null),
    )
}

private val ROWS get() = ChatListSpecimen.LIST.rows
private val LIST get() = ChatListSpecimen.LIST
private val BUBBLES get() = ChatListSpecimen.BUBBLES

/** THE phone Chats screen over the bottom bar (shot `sg_chat-list`). */
@Composable
fun ChatListSpecimenScreen() {
    var filter by remember { mutableStateOf(Chats.ChatFilter.All) }
    AppBackground {
        Box(Modifier.fillMaxSize().testTag(ChatListSpecimenTestId)) {
            Scaffold(containerColor = Color.Transparent) { padding ->
                ChatsContent(
                    list = LIST.copy(
                        rows = ROWS.filter {
                            filter == Chats.ChatFilter.All ||
                                (filter == Chats.ChatFilter.NeedsYou && it.group == SessionTree.ThreadsGroupKey.NeedsYou) ||
                                (filter == Chats.ChatFilter.Running && it.group == SessionTree.ThreadsGroupKey.Running) ||
                                (filter == Chats.ChatFilter.Unread && it.unreadCount > 0)
                        },
                    ),
                    filter = filter,
                    bubbles = BUBBLES,
                    agentOf = { "claude" },
                    onSelectFilter = { filter = it },
                    onOpenChat = {},
                    onLongPressChat = {},
                    onOpenAgent = {},
                    onLongPressAgent = {},
                    onOpenInbox = {},
                    inboxUnread = true,
                    modifier = Modifier.padding(padding),
                )
            }
            SpecimenTabBar(selected = "chats", modifier = Modifier.align(Alignment.BottomCenter))
        }
    }
}

@Composable
private fun SpecimenTabBar(selected: String, modifier: Modifier = Modifier) {
    BottomNavBar(
        agentActive = false,
        boardActive = selected == "board",
        devicesActive = selected == "devices",
        chatsActive = selected == "chats",
        teamActive = selected == "team",
        team = ACME,
        chatsRunning = false,
        chatsNeedYou = true,
        composeEnabled = true,
        onBoard = {},
        onDevices = {},
        onChats = {},
        onTeam = {},
        onCompose = {},
        onChat = {},
        modifier = modifier,
    )
}

/** THE phone tab bar, Chats then Team selected (shot `sg_phone-tab-bar`). */
@Composable
fun PhoneTabBarSpecimenScreen() {
    AppBackground {
        Column(
            modifier = Modifier.fillMaxSize().statusBarsPadding().padding(vertical = 72.dp).testTag(PhoneTabBarSpecimenTestId),
            verticalArrangement = Arrangement.spacedBy(24.dp),
        ) {
            listOf("chats", "team", "board").forEach { tab ->
                Caption("${Chats.Copy.TABS[tab] ?: ACME.name} selected")
                SpecimenTabBar(selected = tab)
            }
        }
    }
}

private val LAUNCH_SURFACE: CardSurface = Json.parseToJsonElement(
    """
    {"root": {"id": "root", "component": "Stack", "props": {"gap": "md"}, "children": [
      {"id": "stat", "component": "StatTile", "props": {"label": "Cold start, Pixel 6a", "value": "740 ms",
        "delta": "−47 %", "tone": "success", "trend": [1400, 1310, 1180, 980, 820, 740]}, "children": []},
      {"id": "actions", "component": "Stack", "props": {"direction": "horizontal", "gap": "sm"}, "children": [
        {"id": "ship", "component": "Button", "props": {"label": "Ship snapshot"}, "children": [],
          "on": {"press": {"event": {"name": "reply", "context": {"text": "Ship the snapshot"}}}}},
        {"id": "keep", "component": "Button", "props": {"label": "Keep profiling", "variant": "secondary"}, "children": [],
          "on": {"press": {"event": {"name": "reply", "context": {"text": "Keep profiling"}}}}}
      ]}
    ]}, "data": {}}
    """.trimIndent(),
).jsonObject.let { CardSurface(it.getValue("root").jsonObject, it["data"] as? JsonObject) }

private fun message(id: String, author: String, body: String) =
    SessionTree.ThreadRow.MessageRow(id = id, author = author, authorId = if (author == "user") "u-dana" else null, body = body, queued = false)

/** The web entry's rows (chat-transcript.tsx) line for line. */
private val TRANSCRIPT: List<SessionTree.ThreadRow> = listOf(
    message("m1", "user", "Cold start is at 1.4 s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of it."),
    SessionTree.ThreadRow.Plan(id = "p-old", sessionId = "r", plan = "# Plan\nProfile first", decision = "superseded"),
    message(
        "m2",
        "agent",
        "Deferred the shape subscribe until after the first frame and added a board snapshot cache. Here's where the time went:",
    ),
    SessionTree.ThreadRow.Surface(id = "s1", sessionId = "r", title = "Launch path", surface = LAUNCH_SURFACE),
    SessionTree.ThreadRow.Divider(id = "d1", kind = Chats.DIVIDER_KIND_COMPACT, at = "2026-10-23T18:20:00.000Z"),
    message("m3", "user", "Anything new overnight?"),
    message("m4", "agent", "Three reports, all about the iPad sign-in. I linked them to #APP-19."),
    SessionTree.ThreadRow.Divider(id = "d2", kind = Chats.DIVIDER_KIND_NEW, at = "2026-10-24T09:30:00.000Z"),
)

private val TIMES = mapOf("m1" to "21:02", "m2" to "21:14", "m3" to "09:02", "m4" to "09:03")

/** A chat's transcript as phone bubbles under THE chat header (shot `sg_chat-transcript`). */
@Composable
fun ChatTranscriptSpecimenScreen() {
    AppBackground {
        Box(Modifier.fillMaxSize().testTag(ChatTranscriptSpecimenTestId)) {
            Scaffold(
                containerColor = Color.Transparent,
                topBar = {
                    ChatHeader(
                        title = "#APP-5 Reduce cold start below 800 ms",
                        subtitle = "${Chats.Copy.PREVIEW_WORKING} · MacBook Pro",
                        subtitleColor = com.exponential.app.ui.issue.NeedsInputAmber,
                        lead = { ChatLeadDisc(Chats.ChatLead.Working, agent = "claude", size = 44.dp) },
                        onBack = {},
                        menu = chatMenuItems(
                            Chats.threadMenuEntries("running", canSteer = true),
                            onStop = {},
                            onDelete = {},
                        ),
                    )
                },
            ) { padding ->
                ThreadConversation(
                    rows = TRANSCRIPT,
                    rootId = "r",
                    meta = { row -> ThreadRowMeta(authorName = "Dana Reyes", time = TIMES[row.id].orEmpty()) },
                    onOpenCard = {},
                    onAnswer = { _, _, _ -> },
                    onDecidePlan = { _, _, _ -> },
                    canUnqueue = { false },
                    onUnqueue = {},
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(
                        start = 16.dp,
                        end = 16.dp,
                        top = padding.calculateTopPadding() + 12.dp,
                        bottom = 24.dp,
                    ),
                    onSurfaceReply = {},
                )
            }
        }
    }
}

/** THE chat menu as a phone presents it (chats-menu.png): the long-press
 *  sheet titled with the chat's title over the list — a live chat's Stop ·
 *  Stop and delete (shot `sg_chat-menus`). */
@Composable
fun ChatMenusSpecimenScreen() {
    Box(Modifier.fillMaxSize().testTag(ChatMenusSpecimenTestId)) {
        ChatListSpecimenScreen()
    }
    com.exponential.app.ui.components.ChatMenuSheet(
        title = "Spike: offline queue",
        items = chatMenuItems(Chats.threadMenuEntries("running", true), {}, {}),
        onDismiss = {},
    )
}

/** The schedule picker with the new `minutes` repeat (shot
 *  `sg_schedule-picker`): Every few minutes · Every 10 minutes, no Time
 *  zone row; then a weekly one with its zone, for contrast. */
@Composable
fun SchedulePickerSpecimenScreen() {
    var minutes by remember {
        mutableStateOf(
            ScheduleDraft(
                schedule = Schedule(repeat = SchedulePicker.REPEAT_MINUTES, minute = 0, hour = 0, weekdays = emptyList(), dayOfMonth = 1, every = 10),
                cron = "",
                tz = "Europe/Vienna",
            ),
        )
    }
    var weekly by remember {
        mutableStateOf(
            ScheduleDraft(
                schedule = Schedule(repeat = SchedulePicker.REPEAT_WEEKLY, minute = 0, hour = 9, weekdays = listOf(1, 2, 3, 4, 5), dayOfMonth = 1),
                cron = "",
                tz = "Europe/Vienna",
            ),
        )
    }
    AppBackground {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .verticalScroll(rememberScrollState())
                .padding(vertical = 56.dp)
                .testTag(SchedulePickerSpecimenTestId),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Caption("${SchedulePicker.Copy.REPEAT.getValue(SchedulePicker.REPEAT_MINUTES)} · ${SchedulePicker.scheduleSummary("*/10 * * * *", "Europe/Vienna")}")
            ScheduleFieldsSpecimen(minutes) { minutes = it }
            Caption(SchedulePicker.scheduleSummary("0 9 * * 1-5", "Europe/Vienna"))
            ScheduleFieldsSpecimen(weekly) { weekly = it }
        }
    }
}

@Composable
private fun Caption(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
    )
}
