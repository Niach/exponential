package com.exponential.app.ui.styleguide

import com.exponential.app.ui.components.ChatRow
import com.exponential.app.domain.SealedPlaceholder
import com.exponential.app.data.sealed.SealedContent
import androidx.compose.foundation.layout.height
import androidx.compose.ui.graphics.graphicsLayer
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PageHeaderActions
import com.exponential.app.ui.components.PhoneBoardMenu
import com.exponential.app.ui.theme.glassRow
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
import com.exponential.app.ui.components.ChatMenuSheetFace
import com.exponential.app.ui.components.ChatSelectionBar
import com.exponential.app.ui.components.SelectionBarSlot
import com.exponential.app.ui.components.BottomNavDefaults
import androidx.compose.foundation.layout.navigationBarsPadding
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
// entries `chat-list`, `phone-tab-bar`, `phone-board-menu`, `phone-page-header`,
// `chat-transcript`, `chat-menus` and
// the updated `schedule-picker` — capture-only screens (no server, no
// sign-in) drawn by the REAL composables over specimen data, shot by
// StyleguideScreenshotsTest#captureOnlyScreens.

const val ChatListSpecimenTestId = "chat-list-specimen"
const val PhoneTabBarSpecimenTestId = "phone-tab-bar-specimen"
const val PhoneBoardMenuSpecimenTestId = "phone-board-menu-specimen"
const val PhonePageHeaderSpecimenTestId = "phone-page-header-specimen"
const val ChatTranscriptSpecimenTestId = "chat-transcript-specimen"
const val ChatMenusSpecimenTestId = "chat-menus-specimen"
const val ChatSelectionSpecimenTestId = "chat-selection-specimen"
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
    dim: Boolean = false,
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
    dim = dim,
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
                activityAt = "2026-10-22T10:00:00.000Z", dim = true,
            ),
        ),
        counts = Chats.ChatCounts(needsYou = 1, running = 1, unread = 1),
    )

    val BUBBLES: List<Chats.AgentBubble> = listOf(
        // S-3: the viewer's Assistant first (no conversation yet = no ring).
        Chats.AgentBubble(SessionTree.AgentRow("as", "t1", "Assistant", "bot", "#6366f1", busy = false, needsYou = false, assistant = true), null),
        Chats.AgentBubble(SessionTree.AgentRow("ag1", "t1", "Triage", "bug", "#a855f7", busy = false, needsYou = true), Chats.AgentRing.NeedsYou),
        Chats.AgentBubble(SessionTree.AgentRow("ag2", "t1", "Designer", "bot", "#6366f1", busy = false, needsYou = false), Chats.AgentRing.Unread),
        Chats.AgentBubble(SessionTree.AgentRow("ag3", "t1", "Release", "package", "#22c55e", busy = false, needsYou = false), null),
    )

    /** S-3: the pinned AGENT rows = `agentRowCases[0].expected` (the
     *  Assistant first with the brand mark, Triage busy, Designer needs you). */
    val AGENT_ROWS: List<Chats.AgentChatRow> = listOf(
        Chats.AgentChatRow(
            agentId = "as", sessionId = "c-as", name = "Assistant", icon = "bot", color = "#6366f1", assistant = true,
            preview = Chats.ChatPreview("agent", "All chats are on track."), time = "09:00",
            activityAt = "2026-10-23T09:00:00.000Z", unreadCount = 1, needsYou = false, busy = false,
        ),
        Chats.AgentChatRow(
            agentId = "ag1", sessionId = "c-ag1", name = "Triage", icon = "bug", color = "#a855f7", assistant = false,
            preview = Chats.ChatPreview("working", Chats.Copy.PREVIEW_WORKING), time = "08:00",
            activityAt = "2026-10-23T08:00:00.000Z", unreadCount = 0, needsYou = false, busy = true,
        ),
        Chats.AgentChatRow(
            agentId = "ag2", sessionId = "c-ag2", name = "Designer", icon = "bot", color = "#6366f1", assistant = false,
            preview = Chats.ChatPreview("question", "Question: Which palette?"), time = "09:30",
            activityAt = "2026-10-23T09:30:00.000Z", unreadCount = 1, needsYou = true, busy = false,
        ),
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
                    // S-3 (stories.json specimen): Triage + Designer wear
                    // their rings, the two chat rings follow.
                    rings = STORY_RING_SPECIMEN,
                    ringAvatar = ::storySpecimenAvatar,
                    // S-3: the pinned Agents section, filtered by the chip like a chat.
                    agentRows = ChatListSpecimen.AGENT_ROWS.filter {
                        when (filter) {
                            Chats.ChatFilter.All -> true
                            Chats.ChatFilter.NeedsYou -> it.needsYou
                            Chats.ChatFilter.Running -> it.busy
                            Chats.ChatFilter.Unread -> it.unreadCount > 0
                        }
                    },
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

/** #EXP-1388 the specimen's archived count (the Archived entry's slot) and
 *  selection (iOS ChatListSpecimen line for line): `a` live (needs you),
 *  `c` ended with one unread, both the viewer's. */
const val CHAT_SELECTION_ARCHIVED = 2
val CHAT_SELECTION_IDS: List<String> = listOf("a", "c")

/** The selection bar's actions for the specimen's selection. */
fun chatSelectionSpecimenActions(): List<Chats.ChatSelectionEntry> = Chats.chatSelectionActions(
    CHAT_SELECTION_IDS.map { id ->
        val row = ROWS.first { it.sessionId == id }
        Chats.ChatSelectionRow(
            sessionId = id,
            rootStatus = if (row.group == SessionTree.ThreadsGroupKey.Recent) "ended" else "running",
            unreadCount = row.unreadCount,
            canSteer = true,
            canDelete = true,
        )
    },
)

/** THE Chats list in selection mode: the Archived entry first, listCases[0]
 *  with two rows selected (nothing shifts: the full-width tint + the select
 *  badge on the stronger disc), THE BAR (the bare count) in the tab bar's
 *  slot (shot `sg_chat-selection`). */
@Composable
fun ChatSelectionSpecimenScreen() {
    var selected by remember { mutableStateOf(CHAT_SELECTION_IDS) }
    AppBackground {
        Box(Modifier.fillMaxSize().testTag(ChatSelectionSpecimenTestId)) {
            Scaffold(containerColor = Color.Transparent) { padding ->
                ChatsContent(
                    list = LIST.copy(archived = CHAT_SELECTION_ARCHIVED),
                    filter = Chats.ChatFilter.All,
                    bubbles = BUBBLES,
                    agentOf = { "claude" },
                    onSelectFilter = {},
                    onOpenChat = {},
                    onLongPressChat = {},
                    onOpenAgent = {},
                    onLongPressAgent = {},
                    onOpenInbox = {},
                    inboxUnread = true,
                    selected = selected,
                    onToggleSelect = { id -> selected = if (id in selected) selected - id else selected + id },
                    modifier = Modifier.padding(padding),
                )
            }
            // THE issue list's slot (outside the Scaffold: + the nav-bar inset).
            SelectionBarSlot(Modifier.navigationBarsPadding()) {
                ChatSelectionBar(
                    count = CHAT_SELECTION_IDS.size,
                    actions = chatSelectionSpecimenActions(),
                    onClear = {},
                    onAction = {},
                )
            }
        }
    }
}

// #EXP-1384: the web specimen's boards (chat-specimens.tsx BOARDS).
private fun specimenBoard(id: String, name: String, icon: String, color: String, order: Double) = BoardEntity(
    id = id, teamId = "t1", name = name, slug = id, prefix = id.uppercase(), color = color, icon = icon,
    sortOrder = order, createdAt = "", updatedAt = "",
)

private val SPECIMEN_BOARDS = listOf(
    specimenBoard("app", "Mobile App", "code", "#6366f1", 1.0),
    specimenBoard("mkt", "Launch Marketing", "square-kanban", "#f59e0b", 2.0),
    specimenBoard("fb", "Product Feedback", "megaphone", "#22c55e", 3.0),
)

@Composable
private fun SpecimenTabBar(
    selected: String,
    modifier: Modifier = Modifier,
    dragging: Boolean = false,
    menuOpen: Boolean = false,
) {
    BottomNavBar(
        selected = selected,
        agentActive = false,
        board = SPECIMEN_BOARDS.first(),
        onBoardView = selected == "board",
        team = ACME,
        chatsRunning = false,
        chatsNeedYou = true,
        composeEnabled = true,
        onSelectTab = {},
        onCompose = {},
        onChat = {},
        modifier = modifier,
        boardMenu = PhoneBoardMenu(
            boards = SPECIMEN_BOARDS,
            activeBoardId = SPECIMEN_BOARDS.first().id,
            onSelect = {},
            onNewBoard = {},
        ),
        menuOpen = menuOpen,
        // The web specimen's static drag: the lens over Apps, the finger
        // 10 left of its centre.
        lensPreview = if (dragging) "apps" else null,
        lensPreviewOffset = if (dragging) (-10).dp else 0.dp,
    )
}

/** THE phone tab bar: on the board view (the caret), Chats, Team, and the
 *  drag lens (shot `sg_phone-tab-bar`). */
@Composable
fun PhoneTabBarSpecimenScreen() {
    AppBackground {
        Column(
            modifier = Modifier.fillMaxSize().statusBarsPadding().padding(vertical = 48.dp).testTag(PhoneTabBarSpecimenTestId),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Caption("On the board view (the caret opens the board menu)")
            SpecimenTabBar(selected = "board")
            Caption("Chats selected")
            SpecimenTabBar(selected = "chats")
            Caption("Team selected")
            SpecimenTabBar(selected = "team")
            Caption("Dragging: the lens between Chats and Apps")
            SpecimenTabBar(selected = "chats", dragging = true)
        }
    }
}

/** THE board menu, OPEN over the strip it grew from (shot `sg_phone-board-menu`). */
@Composable
fun PhoneBoardMenuSpecimenScreen() {
    AppBackground {
        Box(Modifier.fillMaxSize().testTag(PhoneBoardMenuSpecimenTestId)) {
            Column(
                modifier = Modifier.fillMaxWidth().statusBarsPadding().graphicsLayer { alpha = 0.4f },
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                PageHeader(title = SPECIMEN_BOARDS.first().name, actions = PageHeaderActions({}, {}, inboxUnread = false))
                repeat(3) {
                    Box(
                        Modifier
                            .padding(horizontal = 16.dp)
                            .fillMaxWidth()
                            .height(44.dp)
                            .glassRow(),
                    )
                }
            }
            SpecimenTabBar(selected = "board", menuOpen = true, modifier = Modifier.align(Alignment.BottomCenter))
        }
    }
}

/** THE page header: the inbox dot on and off, a pushed page's back control
 *  (shot `sg_phone-page-header`). */
@Composable
fun PhonePageHeaderSpecimenScreen() {
    AppBackground {
        Column(
            modifier = Modifier.fillMaxSize().statusBarsPadding().padding(vertical = 24.dp).testTag(PhonePageHeaderSpecimenTestId),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Caption("Unread notification (the inbox dot)")
            PageHeader(title = Chats.Copy.HEADERS.getValue("chats"), actions = PageHeaderActions({}, {}, inboxUnread = true))
            Caption("All read")
            PageHeader(title = SPECIMEN_BOARDS.first().name, actions = PageHeaderActions({}, {}, inboxUnread = false))
            Caption("Pushed (Devices, the back control)")
            PageHeader(
                title = Chats.Copy.HEADERS.getValue("devices"),
                onBack = {},
                actions = PageHeaderActions({}, {}, inboxUnread = false),
            )
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
                            Chats.threadMenuEntries("running", canSteer = true, canDelete = true),
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

/** THE chat menus as the phone's long-press sheets, in the web entry's
 *  order (chat-menus.tsx; iOS's faces line for line): a chat (S-3: Delete
 *  only, no Stop), the agent menu of an owner (Compact conversation · New
 *  conversation | Agent settings) (shot `sg_chat-menus`). */
@Composable
fun ChatMenusSpecimenScreen() {
    AppBackground {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 24.dp)
                .testTag(ChatMenusSpecimenTestId),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            ChatMenuSheetFace("Spike: offline queue", chatMenuItems(Chats.threadMenuEntries("running", true, true)) {})
            ChatMenuSheetFace("Triage", agentMenuItems(Chats.agentMenuEntries(busy = true, isOwner = true)) {})
        }
    }
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

// ── sealed-locked ────────────────────────────────────────────────────────
// EXP-1459 (sealed-content.json `placeholder`): content this install cannot
// open (yet), the web entry's rows (chat-specimens.tsx SealedLockedSpecimen)
// line for line: in the transcript THE pill with the lock, in the list the
// preview line italic muted, like `working`.

const val SealedLockedSpecimenTestId = "sealed-locked-specimen"

private val LOCKED_TRANSCRIPT: List<SessionTree.ThreadRow> = listOf(
    message("l-m1", "user", "Cold start is at 1.4 s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of it."),
    SessionTree.ThreadRow.Locked(id = "l-k1", reason = SealedContent.LOCKED_NO_KEY, author = "agent"),
    message("l-m2", "agent", "Deferred the shape subscribe until after the first frame."),
    SessionTree.ThreadRow.Locked(id = "l-k2", reason = SealedContent.LOCKED_UNREADABLE, author = "user"),
)

private val LOCKED_LIST_ROWS: List<Chats.ChatRow> get() = LIST.rows.take(3).mapIndexed { index, row ->
    when (index) {
        0 -> row.copy(preview = Chats.ChatPreview(Chats.PREVIEW_KIND_LOCKED, SealedPlaceholder.COPY_NO_KEY))
        2 -> row.copy(preview = Chats.ChatPreview(Chats.PREVIEW_KIND_LOCKED, SealedPlaceholder.COPY_UNREADABLE))
        else -> row
    }
}

/** The locked state: a transcript with both pills over three chat rows with locked previews (shot `sg_sealed-locked`). */
@Composable
fun SealedLockedSpecimenScreen() {
    AppBackground {
        Column(
            Modifier
                .fillMaxSize()
                .testTag(SealedLockedSpecimenTestId)
                .statusBarsPadding()
                .padding(top = 12.dp),
        ) {
            ThreadConversation(
                rows = LOCKED_TRANSCRIPT,
                rootId = "r",
                meta = { row -> ThreadRowMeta(authorName = "Dana Reyes", time = TIMES[row.id.removePrefix("l-")].orEmpty()) },
                onOpenCard = {},
                onAnswer = { _, _, _ -> },
                onDecidePlan = { _, _, _ -> },
                canUnqueue = { false },
                onUnqueue = {},
                modifier = Modifier.fillMaxWidth().weight(1f),
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 12.dp),
                onSurfaceReply = {},
            )
            Column(Modifier.fillMaxWidth().padding(bottom = 24.dp)) {
                LOCKED_LIST_ROWS.forEach { row ->
                    ChatRow(row = row, agent = "claude", onClick = {}, onLongClick = null)
                }
            }
        }
    }
}
