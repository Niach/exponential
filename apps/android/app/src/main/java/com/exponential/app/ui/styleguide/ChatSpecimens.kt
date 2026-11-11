package com.exponential.app.ui.styleguide

import com.exponential.app.ui.components.ChatRow
import com.exponential.app.domain.SealedPlaceholder
import com.exponential.app.data.sealed.SealedContent
import androidx.compose.foundation.layout.height
import androidx.compose.ui.graphics.graphicsLayer
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PageHeaderActions
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import coil3.SingletonImageLoader
import coil3.asImage
import coil3.memory.MemoryCache
import com.exponential.app.domain.TeamImage
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.PhoneBoardMenu
import com.exponential.app.ui.components.TeamAvatar
import com.exponential.app.ui.components.TeamImageRow
import com.exponential.app.ui.theme.glassGroup
import com.exponential.app.ui.theme.glassRow
import androidx.compose.foundation.layout.Row
import androidx.compose.ui.platform.LocalContext
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.consumeWindowInsets
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
import androidx.compose.ui.layout.layout
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
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassAlertFace
import androidx.compose.ui.unit.sp
import com.exponential.app.ui.components.ChatSelectionBar
import com.exponential.app.ui.components.SelectionBarSlot
import com.exponential.app.ui.components.BottomNavDefaults
import androidx.compose.foundation.layout.navigationBarsPadding
import com.exponential.app.ui.components.ScheduleFieldsSpecimen
import com.exponential.app.ui.components.agentMenuItems
import com.exponential.app.ui.components.chatMenuItems
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis
import androidx.compose.foundation.background
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.draw.clip
import com.exponential.app.ui.components.ChatLeadDisc
import com.exponential.app.ui.components.ChatMenuItem
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.chat.AgentGlyph
import com.exponential.app.ui.chat.ChatHeader
import com.exponential.app.ui.chat.ChatHeaderButton
import com.exponential.app.ui.chat.ChatConversation
import com.exponential.app.ui.chat.ChatRowMeta
import com.exponential.app.ui.chats.ChatsContent
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

// Session 58cb32df (fixtures/chats.json): the Android twins of the styleguide
// entries `chat-list`, `phone-tab-bar`, `phone-board-menu`, `phone-page-header`,
// `chat-header`,
// `chat-transcript`, `chat-menus` and
// the updated `schedule-picker` — capture-only screens (no server, no
// sign-in) drawn by the REAL composables over specimen data, shot by
// StyleguideScreenshotsTest#captureOnlyScreens.

const val ChatListSpecimenTestId = "chat-list-specimen"
const val PhoneTabBarSpecimenTestId = "phone-tab-bar-specimen"
const val PhoneBoardMenuSpecimenTestId = "phone-board-menu-specimen"
const val TeamAvatarSpecimenTestId = "team-avatar-specimen"
const val PhonePageHeaderSpecimenTestId = "phone-page-header-specimen"
const val ChatHeaderSpecimenTestId = "chat-header-specimen"
const val ChatTranscriptSpecimenTestId = "chat-transcript-specimen"
const val ChatMenusSpecimenTestId = "chat-menus-specimen"
const val ChatSelectionSpecimenTestId = "chat-selection-specimen"
const val SchedulePickerSpecimenTestId = "schedule-picker-specimen"

private val ACME = TeamEntity(id = "t1", name = "Acme", slug = "acme", createdAt = "", updatedAt = "")

private fun chatRow(
    id: String,
    title: String,
    group: SessionTree.ChatsGroupKey,
    lead: Chats.ChatLead,
    previewKind: String,
    preview: String,
    time: String,
    unread: Int = 0,
    needsYou: Boolean = false,
    busy: Boolean = false,
    word: SessionTree.ChatRowWord? = null,
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
 * .expected` (four rows), what the web entry and the desktop
 * twin render. The app has no fixture at runtime, so the rows are spelled
 * here and `ChatSpecimenFixtureTest` proves them equal to the fixture's.
 */
object ChatListSpecimen {
    val LIST: Chats.ChatList = Chats.ChatList(
        rows = listOf(
            chatRow(
                "b", "Running one", SessionTree.ChatsGroupKey.Running, Chats.ChatLead.Working,
                "working", Chats.Copy.PREVIEW_WORKING, "09:25", busy = true,
                activityAt = "2026-10-23T09:25:00.000Z",
            ),
            chatRow(
                "a", "Needs you one", SessionTree.ChatsGroupKey.NeedsYou, Chats.ChatLead.NeedsYou,
                "question", "Question: Snapshot?", "09:10", needsYou = true, busy = true,
                activityAt = "2026-10-23T09:10:00.000Z", deviceId = "dev-mac",
            ),
            chatRow(
                "c", "Open PR", SessionTree.ChatsGroupKey.Recent, Chats.ChatLead.Open,
                "agent", "Waiting for review", "08:30", unread = 1, word = SessionTree.ChatRowWord.ReadyToMerge,
                activityAt = "2026-10-23T08:30:00.000Z",
            ),
            chatRow(
                "d", "Merged", SessionTree.ChatsGroupKey.Recent, Chats.ChatLead.Merged,
                "you", "You: ship it", Chats.Copy.TIME_YESTERDAY,
                activityAt = "2026-10-22T10:00:00.000Z", dim = true,
            ),
        ),
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

    /** #EXP-1733: the APP rows = `appRowCases[0].expected` (by name: Budget
     *  with no session, Triage board needs you, working hours busy). */
    val APP_ROWS: List<Chats.AppChatRow> = listOf(
        Chats.AppChatRow(
            appId = "ap1", slug = "budget", sessionId = null, name = "Budget", icon = null, color = null,
            preview = Chats.ChatPreview("empty", Chats.Copy.PREVIEW_EMPTY), time = "",
            activityAt = null, unreadCount = 0, needsYou = false, busy = false,
        ),
        Chats.AppChatRow(
            appId = "ap3", slug = "triage-board", sessionId = "e-ap3", name = "Triage board", icon = "bug", color = "#a855f7",
            preview = Chats.ChatPreview("question", "Question: Which columns?"), time = "09:40",
            activityAt = "2026-10-23T09:40:00.000Z", unreadCount = 1, needsYou = true, busy = false,
        ),
        Chats.AppChatRow(
            appId = "ap2", slug = "working-hours", sessionId = "e-ap2", name = "working hours", icon = "clock", color = "#22c55e",
            preview = Chats.ChatPreview("working", Chats.Copy.PREVIEW_WORKING), time = "09:30",
            activityAt = "2026-10-23T09:30:00.000Z", unreadCount = 1, needsYou = false, busy = true,
        ),
    )
}

private val ROWS get() = ChatListSpecimen.LIST.rows
private val LIST get() = ChatListSpecimen.LIST

/** THE phone Chats screen over the bottom bar (shot `sg_chat-list`, #FEED-191:
 *  the Agents section, #EXP-1733 the Apps section, then the chats grouped; the Needs you entry is the
 *  `needs-you` specimen's, as on web and iOS). */
@Composable
fun ChatListSpecimenScreen() {
    AppBackground {
        Box(Modifier.fillMaxSize().testTag(ChatListSpecimenTestId)) {
            Scaffold(containerColor = Color.Transparent) { padding ->
                ChatsContent(
                    list = LIST,
                    agentRows = ChatListSpecimen.AGENT_ROWS,
                    appRows = ChatListSpecimen.APP_ROWS,
                    agentOf = { "claude" },
                    onOpenChat = {},
                    onLongPressChat = {},
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
val CHAT_SELECTION_IDS: List<String> = listOf("b", "c")

/** The selection bar's actions for the specimen's selection. */
fun chatSelectionSpecimenActions(): List<Chats.ChatSelectionEntry> = Chats.chatSelectionActions(
    CHAT_SELECTION_IDS.map { id ->
        val row = ROWS.first { it.sessionId == id }
        Chats.ChatSelectionRow(
            sessionId = id,
            rootStatus = if (row.group == SessionTree.ChatsGroupKey.Recent) "ended" else "running",
            unreadCount = row.unreadCount,
            canSteer = true,
            canDelete = true,
        )
    },
)

/** THE Chats list in selection mode under the Agents section (EXP-1553):
 *  the Archived entry first under its header, listCases[0]
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
                    // EXP-1553: the Agents section stays while selecting
                    // (agents are never selectable).
                    agentRows = ChatListSpecimen.AGENT_ROWS,
                    agentOf = { "claude" },
                    onOpenChat = {},
                    onLongPressChat = {},
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
        onTeamPage = selected == "team",
        chatsRunning = false,
        chatsNeedYou = true,
        composeEnabled = true,
        onSelectTab = {},
        onCompose = {},
        onChat = {},
        modifier = modifier,
        boardMenu = PhoneBoardMenu.boards(
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
            // EXP-1554: the bars sit mid-screen, so none of them adds the
            // navigation-bar inset under itself (it opened a gesture-bar-tall
            // gap between each bar and the next caption, iOS/web-mobile have none).
            modifier = Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .consumeWindowInsets(WindowInsets.navigationBars)
                .padding(vertical = 48.dp)
                .testTag(PhoneTabBarSpecimenTestId),
            verticalArrangement = Arrangement.spacedBy(12.dp),
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

/** The styleguide `team-avatar` entry (#EXP-1645, team-image.json): the marks
 *  with an image and without, then the General row both ways (shot
 *  `sg_team-avatar`). */
@Composable
fun TeamAvatarSpecimenScreen() {
    val context = LocalContext.current
    remember(context) {
        seedTeamSampleImage(context)
        true
    }
    AppBackground {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .padding(16.dp)
                .testTag(TeamAvatarSpecimenTestId),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                TeamAvatar(TEAM_SAMPLE_IMAGE, size = 28.dp)
                TeamAvatar(TEAM_SAMPLE_IMAGE, size = 20.dp)
                TeamAvatar(null, size = 28.dp)
                TeamAvatar(null, size = 20.dp)
                TeamAvatar(null, size = 18.dp)
            }
            Column(Modifier.fillMaxWidth().glassGroup()) {
                TeamImageRow(iconUrl = TEAM_SAMPLE_IMAGE, onPickPhoto = {}, onPickFile = {}, onRemove = {})
                GroupDivider()
                TeamImageRow(iconUrl = null, onPickPhoto = {}, onPickFile = {}, onRemove = {})
            }
        }
    }
}

private const val TEAM_SAMPLE_IMAGE = "/api/attachments/team-sample"

/** The web specimen's transparent star (`TEAM_SAMPLE_IMAGE`) at the server's
 *  stored side, in the app ImageLoader's memory cache (no server here). */
private fun seedTeamSampleImage(context: Context) {
    val cache = SingletonImageLoader.get(context).memoryCache ?: return
    val side = TeamImage.Upload.SIDE
    val bitmap = Bitmap.createBitmap(side, side, Bitmap.Config.ARGB_8888)
    val canvas = Canvas(bitmap)
    canvas.scale(side / 24f, side / 24f)
    val star = Path().apply {
        moveTo(12f, 2f)
        lineTo(15f, 9f)
        lineTo(22f, 9f)
        lineTo(16.5f, 13.5f)
        lineTo(18.5f, 21f)
        lineTo(12f, 16.5f)
        lineTo(5.5f, 21f)
        lineTo(7.5f, 13.5f)
        lineTo(2f, 9f)
        lineTo(9f, 9f)
        close()
    }
    canvas.drawPath(star, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFF97316.toInt() })
    cache[MemoryCache.Key(TEAM_SAMPLE_IMAGE)] = MemoryCache.Value(bitmap.asImage())
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

/** #EXP-1688 THE chat header (`shell.chatHeader`, shot `sg_chat-header`): a chat
 *  waiting on you, a working one with a title that truncates, an agent chat
 *  with its telephone: the real [ChatHeader], web's `ChatHeaderSpecimen`. */
@Composable
fun ChatHeaderSpecimenScreen() {
    val discSize = Chats.Shell.ChatHeader.DISC.dp
    val more = listOf(ChatMenuItem(label = "More", icon = ExpIcons.uiMore, onClick = {}))
    AppBackground {
        Column(
            modifier = Modifier.fillMaxSize().statusBarsPadding().padding(vertical = 24.dp).testTag(ChatHeaderSpecimenTestId),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Caption("A chat that needs you (${Chats.Shell.ChatHeader.ROW}px, ghost buttons, always leading)")
            ChatHeader(
                title = "Reduce cold start",
                subtitle = "${SessionTree.ChatRowWord.NeedsInput.text} · Alex's MacBook",
                subtitleColor = NeedsInputAmber,
                lead = { ChatLeadDisc(Chats.ChatLead.NeedsYou, agent = "claude", size = discSize) },
                onBack = {},
                menu = more,
            )
            Caption("A working chat")
            ChatHeader(
                title = "A title long enough that it truncates on one line",
                subtitle = Chats.Copy.PREVIEW_WORKING,
                subtitleColor = MaterialTheme.colorScheme.onSurface,
                lead = { ChatLeadDisc(Chats.ChatLead.Working, agent = "claude", size = discSize) },
                onBack = {},
                menu = more,
            )
            Caption("An agent chat (the telephone)")
            ChatHeader(
                title = "Triage",
                subtitle = SessionTree.agentChatSubtitle(null),
                lead = {
                    Box(
                        Modifier.size(discSize).clip(CircleShape).background(DesignTokens.Palette.Muted, CircleShape),
                        contentAlignment = Alignment.Center,
                    ) {
                        AgentGlyph(icon = "bug", color = "#a855f7", size = Chats.Shell.ChatHeader.DISC_GLYPH.dp)
                    }
                },
                onBack = {},
                menu = more,
                call = { ChatHeaderButton(ExpIcons.voiceCall, "Call", onClick = {}) },
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

private fun message(id: String, author: String, body: String, authorId: String? = if (author == "user") SPECIMEN_MEMBER else null) =
    SessionTree.SessionChatRow.MessageRow(id = id, author = author, authorId = authorId, body = body, queued = false)

/** #EXP-1736: the transcript's viewer and the member writing beside them. */
private const val SPECIMEN_VIEWER = "u-alex"
private const val SPECIMEN_MEMBER = "u-dana"

/** #EXP-1736 (transcript-grouping.json): the transcript's own sender, a plain
 *  session (its agent's bubbles wear its title + the session avatar). */
private val SPECIMEN_CHAT = com.exponential.app.domain.TranscriptGrouping.Chat(
    sessionId = "r",
    title = "Reduce cold start",
    agentId = null,
    originSessionId = null,
)

/** The web entry's rows (chat-transcript.tsx), #EXP-1736 grouped: a member
 *  (avatar + name), the agent's group of two, a child's report (`›`), the
 *  viewer's own bubble. */
private val TRANSCRIPT: List<SessionTree.SessionChatRow> = listOf(
    message("m1", "user", "Cold start is at 1.4 s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of it."),
    SessionTree.SessionChatRow.Plan(id = "p-old", sessionId = "r", plan = "# Plan\nProfile first", decision = "superseded"),
    message(
        "m2",
        "agent",
        "Deferred the shape subscribe until after the first frame and added a board snapshot cache.",
    ),
    message("m2b", "agent", "Here's where the time went:"),
    SessionTree.SessionChatRow.Surface(id = "s1", sessionId = "r", title = "Launch path", surface = LAUNCH_SURFACE),
    // `specimenClock` reads UTC: 18:20 / 09:30 like web, desktop and iOS.
    SessionTree.SessionChatRow.Divider(id = "d1", kind = Chats.DIVIDER_KIND_COMPACT, at = "2026-10-23T18:20:00.000Z"),
    SessionTree.SessionChatRow.Report(
        id = "rep1",
        sessionId = "c1",
        text = "Report from Measure on the Galaxy A54",
        summary = "Cold start is 0.9 s on the Galaxy A54 with the snapshot cache.",
        xref = SessionTree.SessionXref("report", "c1", "Measure on the Galaxy A54", null),
    ),
    message("m3", "user", "Anything new overnight?", authorId = SPECIMEN_VIEWER),
    message("m4", "agent", "Three reports, all about the iPad sign-in. I linked them to #APP-19."),
    // #EXP-1772: a busy agent chat: a schedule firing the agent answered
    // (THE marker above the reply), a merge line, a relay (its name above).
    SessionTree.SessionChatRow.Event(id = "e1", event = "trigger_fired", text = "Schedule", sessionId = null),
    message("m6", "agent", "PR #412 merged overnight; the iPad sign-in reports stopped."),
    SessionTree.SessionChatRow.Event(id = "e2", event = "pr_merged", text = "Merged PR #412", sessionId = null),
    SessionTree.SessionChatRow.MessageRow(
        id = "x2",
        author = "agent",
        authorId = null,
        body = "Verified on the iPad: sign-in keeps the return URL, and the redirect loop is gone on Safari too.",
        queued = false,
        authorSessionId = "c-fix",
        xref = SessionTree.SessionXref("message", "c-fix", "Fix the sign-in redirect", null),
    ),
    SessionTree.SessionChatRow.Divider(id = "d2", kind = Chats.DIVIDER_KIND_NEW, at = "2026-10-24T09:30:00.000Z"),
)

private val TIMES = mapOf("m1" to "21:02", "m2" to "21:14", "m2b" to "21:14", "m3" to "09:02", "m4" to "09:03", "m6" to "07:00", "x2" to "08:41")

/** The specimen's names by member id. */
private fun specimenAuthor(row: SessionTree.SessionChatRow.MessageRow): String =
    if (row.authorId == SPECIMEN_VIEWER) "Alex" else "Dana Reyes"

/** The specimen's wall clock: the bubbles' times above are the web entry's
 *  UTC wall clock, so the divider pills format in the same zone (EXP-1553). */
private fun specimenClock(wire: String): String? = runCatching {
    java.time.Instant.parse(wire).atZone(java.time.ZoneOffset.UTC)
        .format(java.time.format.DateTimeFormatter.ofPattern("HH:mm"))
}.getOrNull()

/** A chat's transcript as phone bubbles, bare (no header: the web entry's
 *  picture; shot `sg_chat-transcript`). */
@Composable
fun ChatTranscriptSpecimenScreen() {
    AppBackground {
        Box(Modifier.fillMaxSize().statusBarsPadding().testTag(ChatTranscriptSpecimenTestId)) {
            ChatConversation(
                rows = TRANSCRIPT,
                rootId = "r",
                meta = { row -> ChatRowMeta(authorName = specimenAuthor(row), time = TIMES[row.id].orEmpty()) },
                onOpenCard = {},
                onAnswer = { _, _, _ -> },
                onDecidePlan = { _, _, _ -> },
                canUnqueue = { false },
                onUnqueue = {},
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 24.dp),
                onSurfaceReply = {},
                onOpenXref = {},
                clock = ::specimenClock,
                chat = SPECIMEN_CHAT,
                viewerId = SPECIMEN_VIEWER,
            )
        }
    }
}

/** THE chat menus as the phone's long-press sheets, web's set and captions
 *  (chat-specimens.tsx ChatMenusSpecimen, EXP-1553): a live chat, an ended
 *  chat (S-3: Delete only), the agent menu of the busy owner and of an idle
 *  member, then the delete confirm at rest (shot `sg_chat-menus`). */
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
            MenuCaptioned("A live chat") {
                ChatMenuSheetFace("Spike: offline queue", chatMenuItems(Chats.chatMenuEntries("running", true, true)) {})
            }
            MenuCaptioned("An ended chat") {
                ChatMenuSheetFace("#APP-16 Fix flaky scroll restore", chatMenuItems(Chats.chatMenuEntries("ended", true, true)) {})
            }
            MenuCaptioned("The agent menu (busy, owner)") {
                ChatMenuSheetFace("Triage", agentMenuItems(Chats.agentMenuEntries(busy = true, isOwner = true)) {})
            }
            MenuCaptioned("The agent menu (idle, member)") {
                // Web's specimen names no agent; every phone titles both agent
                // menus with the same agent (iOS: Triage).
                ChatMenuSheetFace("Triage", agentMenuItems(Chats.agentMenuEntries(busy = false, isOwner = false)) {})
            }
            MenuCaptioned("The delete confirm") {
                GlassAlertFace(
                    title = Chats.Copy.CONFIRM_DELETE_TITLE,
                    body = Chats.Copy.CONFIRM_DELETE_BODY,
                    trailing = listOf(
                        GlassAlertAction("Cancel", onClick = {}),
                        GlassAlertAction(Chats.Copy.MENU_DELETE, destructive = true, onClick = {}),
                    ),
                )
            }
        }
    }
}

/** A chat-menus specimen under web's caption (`text-xs` muted, 8dp above). */
@Composable
private fun MenuCaptioned(caption: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            caption,
            style = MaterialTheme.typography.labelSmall.copy(fontSize = 12.sp),
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        content()
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
            // EXP-1553: no captions above the groups on any platform.
            ScheduleFieldsSpecimen(minutes) { minutes = it }
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

private val LOCKED_TRANSCRIPT: List<SessionTree.SessionChatRow> = listOf(
    message("l-m1", "user", "Cold start is at 1.4 s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of it."),
    SessionTree.SessionChatRow.Locked(id = "l-k1", reason = SealedContent.LOCKED_NO_KEY, author = "agent"),
    message("l-m2", "agent", "Deferred the shape subscribe until after the first frame."),
    SessionTree.SessionChatRow.Locked(id = "l-k2", reason = SealedContent.LOCKED_UNREADABLE, author = "user"),
)

private val LOCKED_LIST_ROWS: List<Chats.ChatRow> get() = LIST.rows.take(3).mapIndexed { index, row ->
    when (index) {
        0 -> row.copy(preview = Chats.ChatPreview(Chats.PREVIEW_KIND_LOCKED, SealedPlaceholder.COPY_NO_KEY))
        2 -> row.copy(preview = Chats.ChatPreview(Chats.PREVIEW_KIND_LOCKED, SealedPlaceholder.COPY_UNREADABLE))
        else -> row
    }
}

/** The locked state: a transcript with both pills, the three chat rows with
 *  locked previews right under it (iOS / web order, #EXP-1469: the rows ride
 *  the conversation's tail, never pinned to the bottom; shot `sg_sealed-locked`). */
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
            ChatConversation(
                rows = LOCKED_TRANSCRIPT,
                rootId = "r",
                meta = { row -> ChatRowMeta(authorName = "Dana Reyes", time = TIMES[row.id.removePrefix("l-")].orEmpty()) },
                onOpenCard = {},
                onAnswer = { _, _, _ -> },
                onDecidePlan = { _, _, _ -> },
                canUnqueue = { false },
                onUnqueue = {},
                modifier = Modifier.fillMaxWidth().weight(1f),
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 12.dp),
                onSurfaceReply = {},
                tail = {
                    // The rows span the screen: undo the transcript's 16dp gutters.
                    Column(
                        Modifier
                            .padding(top = 8.dp)
                            .layout { measurable, constraints ->
                                val gutters = (32.dp).roundToPx()
                                val width = constraints.maxWidth + gutters
                                val placeable = measurable.measure(constraints.copy(minWidth = width, maxWidth = width))
                                layout(constraints.maxWidth, placeable.height) { placeable.place(-gutters / 2, 0) }
                            },
                    ) {
                        LOCKED_LIST_ROWS.forEach { row ->
                            ChatRow(row = row, agent = "claude", onClick = {}, onLongClick = null)
                        }
                    }
                },
            )
        }
    }
}
