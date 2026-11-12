package com.exponential.app.ui.apps

import android.util.Log
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.key
import androidx.lifecycle.viewmodel.compose.LocalViewModelStoreOwner
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.PullRequestIssueEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.domain.SessionScreen
import kotlinx.coroutines.flow.MutableStateFlow
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Icon
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.glassGroup
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import at.exponential.ui.host.HostSurface
import at.exponential.ui.host.NavigateTarget
import at.exponential.ui.host.Screens
import com.exponential.app.ui.components.PhoneTabCapsule
import com.exponential.app.ui.components.PhoneTabItem
import com.exponential.app.ui.components.TabPager
import at.exponential.ui.json.JsonValue
import at.exponential.ui.primitives.EmptyStateSize
import com.exponential.app.data.db.AppVersionEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.AppExport
import com.exponential.app.domain.AppTemplates
import com.exponential.app.domain.Apps
import com.exponential.app.domain.AskDock
import com.exponential.app.domain.ComposerMenu
import com.exponential.app.domain.ComposerMenuConditions
import com.exponential.app.domain.ComposerMenuEntry
import com.exponential.app.domain.ComposerMenuRowId
import com.exponential.app.domain.ComposerMenuSurface
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.domain.userMessageTime
import com.exponential.app.ui.components.ChatMenuItem
import com.exponential.app.ui.components.ChatMenuSheet
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.ComposerSubmitButton
import com.exponential.app.ui.components.ComposerToolButton
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.FloatingBarEdge
import com.exponential.app.ui.components.FloatingComposer
import com.exponential.app.ui.components.GlassComposer
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassPillDefaults
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.LocalDetailHaze
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.components.detailHazeSource
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.session.AgentSessionViewModel
import com.exponential.app.ui.agent.ComposerPlusMenuSheet
import com.exponential.app.ui.agent.teamsPagePlus
import com.exponential.app.ui.components.picker.DevicePicker
import com.exponential.app.ui.session.SessionComposer
import com.exponential.app.ui.session.SessionComposerDevice
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.chat.ChatAgent
import com.exponential.app.ui.chat.ChatAnswer
import com.exponential.app.ui.chat.ChatConversation
import com.exponential.app.ui.chat.ChatPlanDecision
import com.exponential.app.ui.chat.ChatRowMeta
import com.exponential.app.ui.chat.ChatUiCopy
import com.exponential.app.ui.chat.ChatViewModel
import com.exponential.app.ui.chat.clockTime
import com.exponential.app.ui.chat.ChatHeader
import com.exponential.app.ui.chat.ChatHeaderButton
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.domain.Chats
import com.exponential.app.ui.components.toPickerAccount
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.components.picker.AccountPicker
import com.exponential.app.ui.components.picker.ModelPicker
import com.exponential.app.ui.components.picker.ModelPickerOption
import com.exponential.app.ui.session.SessionComposerAccount
import com.exponential.app.ui.session.SessionComposerModel
import dev.chrisbanes.haze.rememberHazeState

// VAPP-110 / VAPP-177 / #EXP-1732 (fixtures/session-screen.json + apps.json
// THE EDITOR, web `components/apps/app-editor.tsx`): THE EDITOR on a phone =
// THE session screen of kind `app`: the header band (the app's glyph disc,
// the name over `status · device`, the version pill, THE gear = the app's
// settings as a pushed item, the `…` sheet: Publish while `Apps.canPublish`
// and I may write the app, Export once there is a version, Delete app for the
// creator or an owner) over THE segmented tabs Chat · Preview · Data ·
// Versions · Sessions · Results, and ONE pane. Chat = the member's EDITOR
// SESSION (THE transcript + THE composer; none yet = the empty chat whose
// first send opens it); Preview = `Apps.shownVersion`'s package's first
// template through the app host (eApps V1, #VAPP-210: the head while my
// editor session is live or nothing is published, else the published one);
// Data = the app's records (VAPP-178, `AppDataTab`); Versions = the version
// list (a row's Preview lands on the Preview tab with the viewing banner:
// Publish, Restore = a new head, Current); Sessions = THE tree rows of the
// editor session's tree (a row pushes that session); Results = the tree's
// results. Leaving the editor ENDS the live editor session; the next send
// continues it. VAPP-176: a DRAFT (no version yet) opens on Chat with the
// starters and the composer focused; leaving an empty draft discards it. The
// route keys the app by ID, so a draft's slug following its new name changes
// nothing here. The retired `?tab=settings` opens the gear's settings.

@Composable
fun AppEditorScreen(
    onBack: () -> Unit,
    onOpenSession: (sessionId: String) -> Unit,
    /** #VAPP-230: the upstream banner's Connect (Settings › MCP of the app's team). */
    onOpenTeamSettings: (teamId: String, section: String) -> Unit = { _, _ -> },
    /** VAPP-234: a template picked from the draft's starters opens its editor. */
    onOpenApp: (appId: String) -> Unit = {},
    viewModel: AppEditorViewModel = hiltViewModel(),
) {
    val app by viewModel.app.collectAsStateWithLifecycle()
    val root by viewModel.root.collectAsStateWithLifecycle()
    val versions by viewModel.versions.collectAsStateWithLifecycle()
    // VAPP-200: at app access `read` the preview's writing controls are disabled.
    val access by viewModel.myAccess.collectAsStateWithLifecycle()
    val users by viewModel.users.collectAsStateWithLifecycle()
    val viewing by viewModel.viewing.collectAsStateWithLifecycle()
    val restoring by viewModel.restoring.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    val me by viewModel.me.collectAsStateWithLifecycle()
    val isOwner by viewModel.isOwner.collectAsStateWithLifecycle()
    val deleting by viewModel.deleting.collectAsStateWithLifecycle()
    val toastLine by viewModel.toast.collectAsStateWithLifecycle()
    val records by viewModel.records.collectAsStateWithLifecycle()
    val publishing by viewModel.publishing.collectAsStateWithLifecycle()
    val myAccess by viewModel.myAccess.collectAsStateWithLifecycle()
    val screenRequest by viewModel.screen.collectAsStateWithLifecycle()
    // #EXP-1729 / #EXP-1732: the header's device part and its tone.
    val header by viewModel.header.collectAsStateWithLifecycle()
    val headerDevice by viewModel.headerDeviceLabel.collectAsStateWithLifecycle()
    // eApps R3: Publish needs write access to the app (null = write until known).
    val canWrite = (myAccess ?: "write") == "write"
    var exportOpen by rememberSaveable { mutableStateOf(false) }
    var deleteOpen by rememberSaveable { mutableStateOf(false) }
    // #VAPP-256 (`rollbackCases`): a rollback past a migrated version asks
    // first; every other publish goes through at once.
    var rollbackTarget by rememberSaveable { mutableStateOf<Int?>(null) }
    val requestPublish: (Int) -> Unit = { number ->
        val rows = versions.map { (it.number ?: 0) to it.migrationState }
        if (Apps.rollbackWarns(app?.publishedVersion ?: 0, number, rows)) rollbackTarget = number
        else viewModel.publish(number)
    }
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    val toaster = LocalToaster.current
    LaunchedEffect(toastLine) {
        toastLine?.let {
            toaster.info(it)
            viewModel.clearToast()
        }
    }
    // Leaving the editor ends the live editor session (its whole tree); a
    // rotation recreates the screen without leaving it.
    val activity = androidx.activity.compose.LocalActivity.current
    DisposableEffect(viewModel) {
        onDispose { if (activity?.isChangingConfigurations != true) viewModel.leave() }
    }

    // #EXP-1732 (session-screen.json): the requested tab (the route's `tab`,
    // its retired `versions` / `settings` sheets included) and the opened item.
    var requestedTab by rememberSaveable {
        mutableStateOf<String?>(viewModel.initialTab.sheet ?: viewModel.initialTab.tab)
    }
    var openKind by rememberSaveable { mutableStateOf<String?>(null) }
    var openId by rememberSaveable { mutableStateOf<String?>(null) }
    val closeItem: () -> Unit = {
        openKind = null
        openId = null
        // The retired `?tab=settings` opened it: closing must not reopen it.
        if (requestedTab == SessionScreen.Open.Settings.wire) requestedTab = null
    }
    val entry = app
    val canManage = entry != null && (isOwner || (me != null && entry.createdBy == me))
    val editing = root?.isEnded == false
    val rootId = root?.id
    CompositionLocalProvider(LocalDetailHaze provides rememberHazeState()) {
        AppEditorChat(rootId) { chat ->
            val tree = rememberAppEditorTree(chat)
            // Sessions = EVERY session under the root, off the raw tree.
            val items = remember(tree.sessions, rootId) {
                if (rootId == null) {
                    com.exponential.app.ui.chat.SessionScreenItems.Empty
                } else {
                    com.exponential.app.ui.chat.sessionScreenTreeItems(rootId, tree.sessions.map(SessionTree::session), emptyList())
                }
            }
            // results-tab.json: THE fold, off the main thread (ChatViewModel.results).
            val resultsCount = if (rootId == null) 0 else tree.results.size
            val screen = SessionScreen.screen(
                kind = SessionScreen.Kind.App,
                wide = false,
                requested = requestedTab,
                open = openKind,
                counts = SessionScreen.Counts(sessions = items.sessions.size, results = resultsCount),
            )
            if (screen.layout == SessionScreen.Layout.Item && entry != null) {
                // THE pushed page: `‹` + the item's name; system Back pops it.
                BackHandler(onBack = closeItem)
                Scaffold(
                    containerColor = Color.Transparent,
                    modifier = Modifier.testTag(Apps.TestIds.EDITOR),
                    topBar = {
                        com.exponential.app.ui.chat.ChatHeader(
                            title = when (screen.open) {
                                SessionScreen.Open.Settings -> SessionScreen.Copy.SETTINGS
                                SessionScreen.Open.Session -> openId?.let { items.session(it)?.name }.orEmpty()
                                else -> ""
                            },
                            subtitle = null,
                            lead = {},
                            onBack = closeItem,
                        )
                    },
                ) { padding ->
                    when (screen.open) {
                        SessionScreen.Open.Settings -> Box(Modifier.padding(padding).fillMaxSize()) {
                            AppSettingsBody(
                                app = entry,
                                viewModel = viewModel,
                                records = records.size,
                                canManage = canManage,
                                canGrant = isOwner,
                                onExport = { exportOpen = true },
                                onDelete = { deleteOpen = true },
                            )
                        }
                        SessionScreen.Open.Session -> openId?.let { id ->
                            key(id) {
                                com.exponential.app.ui.components.PageViewModelStore {
                                    com.exponential.app.ui.session.SessionPage(
                                        sessionId = id,
                                        padding = padding,
                                        onOpenIssue = {},
                                        onOpenOrigin = { origin, _ -> onOpenSession(origin) },
                                    )
                                }
                            }
                        }
                        else -> Unit
                    }
                }
                return@AppEditorChat
            }
            Scaffold(
                containerColor = Color.Transparent,
                modifier = Modifier.testTag(Apps.TestIds.EDITOR),
                topBar = {
                    com.exponential.app.ui.chat.ChatHeader(
                        title = entry?.name.orEmpty(),
                        // #EXP-1732 (session-screen.json STATUS WORD): THE
                        // tree's word (never dropped) · apps.json
                        // `editorHeader`'s device part, in its tone.
                        subtitle = if (entry != null) {
                            SessionScreen.subtitle(
                                com.exponential.app.ui.chat.chatStateWord(rootId.orEmpty(), tree.sessions),
                                headerDevice,
                                null,
                            )
                        } else null,
                        subtitleColor = when (header.tone) {
                            Apps.EditorHeaderTone.Attention -> NeedsInputAmber
                            Apps.EditorHeaderTone.Active -> MaterialTheme.colorScheme.onSurface
                            Apps.EditorHeaderTone.Muted -> null
                        },
                        lead = {
                            if (entry != null) {
                                Box(
                                    modifier = Modifier
                                        .size(com.exponential.app.domain.Chats.Shell.ChatHeader.DISC.dp)
                                        .clip(CircleShape)
                                        .background(com.exponential.app.ui.theme.DesignTokens.Palette.Muted, CircleShape),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    AppGlyph(entry.icon, entry.color, size = com.exponential.app.domain.Chats.Shell.ChatHeader.DISC_GLYPH.dp)
                                }
                            }
                        },
                        onBack = onBack,
                        // The version pill (+ VAPP-235's pending offline
                        // writes) right after the name (#EXP-1732, web/iOS/desktop).
                        titleTrailing = if (entry != null) {
                            {
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                                    modifier = Modifier.testTag("app-editor-title"),
                                ) {
                                    AppVersionPill(entry)
                                    AppOutboxPendingGlyph(viewModel.appId)
                                }
                            }
                        } else {
                            null
                        },
                        gear = if (entry != null) {
                            { openKind = SessionScreen.Open.Settings.wire }
                        } else {
                            null
                        },
                        // apps.json `editorTabs.menu` less Versions + Settings
                        // (a tab and the gear now).
                        more = if (entry != null) {
                            {
                                AppEditorMenu(
                                    title = entry.name,
                                    canPublish = canWrite && Apps.canPublish(entry.versions),
                                    onPublish = { viewModel.publish(entry.headVersion) },
                                    canExport = entry.headVersion > 0,
                                    canDelete = canManage,
                                    onExport = { exportOpen = true },
                                    onDelete = { deleteOpen = true },
                                )
                            }
                        } else {
                            null
                        },
                        tabs = {
                            com.exponential.app.ui.chat.SessionScreenTabs(
                                tabs = screen.tabs,
                                active = screen.active,
                                onSelect = { tab ->
                                    requestedTab = tab.wire
                                    openKind = null
                                    openId = null
                                },
                            )
                        },
                    )
                },
            ) { padding ->
                if (entry == null) {
                    Box(Modifier.padding(padding).fillMaxSize()) {
                        SkeletonRows(modifier = Modifier.align(Alignment.TopCenter).padding(16.dp))
                    }
                    return@Scaffold
                }
                val listPadding = PaddingValues(top = padding.calculateTopPadding() + 8.dp, bottom = padding.calculateBottomPadding() + 16.dp)
                when (screen.active) {
                    SessionScreen.Tab.Chat -> AppChatPane(
                        app = entry,
                        rootId = rootId,
                        chat = chat,
                        padding = padding,
                        viewModel = viewModel,
                        onOpenSession = onOpenSession,
                        onOpenApp = onOpenApp,
                    )
                    SessionScreen.Tab.Preview -> {
                        val shown = Apps.shownVersion(entry.versions, editing, viewing?.number)
                        AppPreviewPane(
                            appIcon = entry.icon?.let { ExpIcons.byName(it) } ?: ExpIcons.navApps,
                            packageJson = when (shown) {
                                0 -> null
                                entry.headVersion -> entry.headPackageJson
                                entry.publishedVersion -> entry.packageJson
                                else -> viewing?.packageJson
                            },
                            viewing = viewing?.number,
                            // A grant is about the version it was made on (`grantVersion`):
                            // any other previews asking for everything.
                            grants = shown == Apps.grantVersion(entry.versions),
                            canPublish = canWrite && viewing?.number != entry.publishedVersion,
                            canRestore = canWrite && viewing?.number != entry.headVersion,
                            publishing = publishing,
                            onPublish = { viewing?.number?.let(requestPublish) },
                            restoring = restoring,
                            onRestore = viewModel::restore,
                            onHead = viewModel::clearViewing,
                            onOpenTeamSettings = onOpenTeamSettings,
                            appId = viewModel.appId,
                            teamId = entry.teamId,
                            me = me,
                            readOnly = access == "read",
                            // eApps R8 (apps.json `migrationCases`): a draft preview reads migrated copies.
                            migrateFrom = if (entry.publishedVersion > 0 && shown != entry.publishedVersion) entry.packageJson?.let { runCatching { JsonValue.parse(it)["version"]?.string }.getOrNull() } else null,
                            screen = screenRequest,
                            onScreen = viewModel::requestScreen,
                            modifier = Modifier.padding(padding),
                        )
                    }
                    SessionScreen.Tab.Data -> AppDataTab(
                        app = entry,
                        records = records,
                        viewModel = viewModel,
                        modifier = Modifier.padding(padding),
                    )
                    SessionScreen.Tab.Versions -> AppVersionsList(
                        versions = versions,
                        pointers = entry.versions,
                        users = users,
                        canWrite = canWrite,
                        publishing = publishing,
                        onPreview = { number ->
                            viewModel.pick(number)
                            requestedTab = SessionScreen.Tab.Preview.wire
                        },
                        onPublish = requestPublish,
                        modifier = Modifier
                            .padding(padding)
                            .fillMaxSize()
                            .verticalScroll(rememberScrollState())
                            .padding(horizontal = 16.dp, vertical = 8.dp),
                    )
                    SessionScreen.Tab.Sessions -> com.exponential.app.ui.chat.SessionSessionsBody(
                        sessions = items.sessions,
                        onOpen = { id ->
                            openKind = SessionScreen.Open.Session.wire
                            openId = id
                        },
                        contentPadding = listPadding,
                    )
                    SessionScreen.Tab.Results -> com.exponential.app.ui.chat.ChatResultsPage(
                        rootId = rootId.orEmpty(),
                        results = tree.results,
                        cards = tree.resultCards,
                        onReply = { sessionId, text -> chat?.sendTo(sessionId, text) },
                        pullRequests = tree.pullRequests,
                        teamPullRequests = tree.teamPullRequests,
                        pullRequestIssues = tree.pullRequestIssues,
                        issues = tree.issues,
                        padding = padding,
                        onOpenChat = {},
                    )
                    else -> Unit
                }
            }
        }
    }
    if (exportOpen && entry != null) {
        AppExportSheet(app = entry, records = records, onDismiss = { exportOpen = false })
    }
    rollbackTarget?.let { number ->
        GlassAlert(
            title = Apps.rollbackTitle(number),
            body = Apps.rollbackBody(number),
            onDismiss = { rollbackTarget = null },
            trailing = listOf(
                GlassAlertAction("Cancel", onClick = { rollbackTarget = null }),
                GlassAlertAction(
                    Apps.Copy.PUBLISH,
                    primary = true,
                    testTag = "app-rollback-confirm",
                    onClick = {
                        rollbackTarget = null
                        viewModel.publish(number)
                    },
                ),
            ),
            defaultAction = 0,
        )
    }

    if (deleteOpen && entry != null) {
        GlassAlert(
            title = Apps.deleteTitle(entry.name),
            body = Apps.Copy.DELETE_BODY,
            onDismiss = { if (!deleting) deleteOpen = false },
            trailing = listOf(
                GlassAlertAction("Cancel", onClick = { deleteOpen = false }),
                GlassAlertAction(
                    Apps.Copy.DELETE,
                    destructive = true,
                    loading = deleting,
                    testTag = "app-delete-confirm",
                    onClick = {
                        viewModel.delete {
                            deleteOpen = false
                            closeItem()
                            onBack()
                        }
                    },
                ),
            ),
            defaultAction = 0,
        )
    }
}

/**
 * #EXP-1732: the editor session's ONE [ChatViewModel] (its args = the root's
 * id, [SessionArgsStore]), shared by the Chat, Sessions and Results tabs and
 * the counts; every other view model stays on the editor's own entry. Null
 * while there is no editor session yet.
 */
@Composable
private fun AppEditorChat(rootId: String?, content: @Composable (ChatViewModel?) -> Unit) {
    if (rootId == null) {
        content(null)
        return
    }
    val parent = checkNotNull(LocalViewModelStoreOwner.current) { "AppEditorChat needs a ViewModelStoreOwner" }
    SessionArgsStore(rootId) {
        val chat = hiltViewModel<ChatViewModel>(key = "app-chat:$rootId")
        CompositionLocalProvider(LocalViewModelStoreOwner provides parent) { content(chat) }
    }
}

/** The editor tree's synced rows (empty while there is no editor session). */
private class AppEditorTree(
    val view: SessionTree.ChatView?,
    val sessions: List<SessionEntity>,
    val messages: List<SessionTree.Message>,
    /** results-tab.json: the tree's session-authored card rows. */
    val resultCards: List<SessionTree.Message>,
    /** results-tab.json: THE fold of [resultCards] (ChatViewModel.results). */
    val results: List<com.exponential.app.domain.ResultsTab.Result>,
    val pullRequests: List<PullRequestEntity>,
    val teamPullRequests: List<PullRequestEntity>,
    val pullRequestIssues: List<PullRequestIssueEntity>,
    val issues: List<IssueEntity>,
)

@Composable
private fun rememberAppEditorTree(chat: ChatViewModel?): AppEditorTree {
    val view by (chat?.view ?: remember { MutableStateFlow<SessionTree.ChatView?>(null) }).collectAsStateWithLifecycle()
    val sessions by (chat?.tree ?: remember { MutableStateFlow(emptyList<SessionEntity>()) }).collectAsStateWithLifecycle()
    val messages by (chat?.messages ?: remember { MutableStateFlow(emptyList<SessionTree.Message>()) }).collectAsStateWithLifecycle()
    val resultCards by (chat?.resultCards ?: remember { MutableStateFlow(emptyList<SessionTree.Message>()) }).collectAsStateWithLifecycle()
    val results by (chat?.results ?: remember { MutableStateFlow(emptyList<com.exponential.app.domain.ResultsTab.Result>()) }).collectAsStateWithLifecycle()
    val pullRequests by (chat?.pullRequests ?: remember { MutableStateFlow(emptyList<PullRequestEntity>()) }).collectAsStateWithLifecycle()
    val teamPullRequests by (chat?.teamPullRequests ?: remember { MutableStateFlow(emptyList<PullRequestEntity>()) }).collectAsStateWithLifecycle()
    val pullRequestIssues by (chat?.pullRequestIssues ?: remember { MutableStateFlow(emptyList<PullRequestIssueEntity>()) }).collectAsStateWithLifecycle()
    val issues by (chat?.syncedIssues ?: remember { MutableStateFlow(emptyList<IssueEntity>()) }).collectAsStateWithLifecycle()
    return AppEditorTree(view, sessions, messages, resultCards, results, pullRequests, teamPullRequests, pullRequestIssues, issues)
}

/** VAPP-177 / #EXP-1732: the header's `…` (`editorTabs.menu` less Versions
 *  and Settings, THE Menu's bottom sheet titled by the app): eApps V1 Publish
 *  first (`Apps.canPublish` + write access), Export (a version exists),
 *  Delete app (destructive; the creator or an owner). */
@Composable
private fun AppEditorMenu(
    title: String,
    canPublish: Boolean,
    onPublish: () -> Unit,
    canExport: Boolean,
    canDelete: Boolean,
    onExport: () -> Unit,
    onDelete: () -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    Box {
        com.exponential.app.ui.chat.ChatHeaderButton(
            ExpIcons.uiMore,
            contentDescription = "App actions",
            onClick = { open = true },
            modifier = Modifier.testTag(Apps.TestIds.MENU),
        )
    }
    if (open) {
        ChatMenuSheet(
            title = title,
            onDismiss = { open = false },
            testTag = "app-menu-sheet",
            items = buildList {
                if (canPublish) add(
                    ChatMenuItem(
                        label = Apps.Copy.PUBLISH,
                        icon = ExpIcons.uiSubmit,
                        onClick = {
                            open = false
                            onPublish()
                        },
                        testTag = Apps.TestIds.PUBLISH,
                    ),
                )
                if (canExport) add(
                    ChatMenuItem(
                        label = Apps.Copy.MENU_EXPORT,
                        icon = ExpIcons.uiShare,
                        onClick = {
                            open = false
                            onExport()
                        },
                        testTag = "app-editor-export",
                    ),
                )
                if (canDelete) add(
                    ChatMenuItem(
                        label = Apps.Copy.MENU_DELETE,
                        icon = ExpIcons.uiDelete,
                        onClick = {
                            open = false
                            onDelete()
                        },
                        destructive = true,
                        testTag = "app-editor-delete",
                    ),
                )
            },
        )
    }
}

/** THE preview frame: the viewing banner (Publish / Restore / Current), then
 *  the package's first template through the app host, or the empty state. */
@Composable
private fun AppPreviewPane(
    appIcon: ImageVector,
    packageJson: String?,
    viewing: Int?,
    grants: Boolean,
    canPublish: Boolean,
    canRestore: Boolean,
    publishing: Int?,
    onPublish: () -> Unit,
    restoring: Boolean,
    onRestore: () -> Unit,
    onHead: () -> Unit,
    onOpenTeamSettings: (teamId: String, section: String) -> Unit,
    appId: String,
    teamId: String?,
    me: String?,
    readOnly: Boolean,
    migrateFrom: String? = null,
    screen: AppEditorViewModel.ScreenRequest,
    onScreen: (screen: String, params: Map<String, String>) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (viewing != null) {
            val shape = RoundedCornerShape(GlassTokens.RowRadius)
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
                    .padding(horizontal = 12.dp, vertical = 8.dp)
                    .testTag(Apps.TestIds.VIEWING),
            ) {
                Text(Apps.viewing(viewing), fontWeight = FontWeight.Medium, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
                if (canPublish) {
                    GlassPill(Apps.Copy.PUBLISH, size = PillSize.Sm, primary = true, onClick = onPublish, enabled = publishing == null, modifier = Modifier.testTag("app-viewing-publish"))
                }
                if (canRestore) {
                    GlassPill(Apps.Copy.RESTORE, size = PillSize.Sm, onClick = onRestore, enabled = !restoring, modifier = Modifier.testTag("app-restore"))
                }
                GlassPill(Apps.Copy.HEAD, size = PillSize.Sm, onClick = onHead)
            }
        }
        val shape = RoundedCornerShape(Apps.Preview.RADIUS.dp)
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .clip(shape)
                .background(MaterialTheme.colorScheme.background, shape)
                .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
                .testTag(Apps.TestIds.PREVIEW),
        ) {
            val pkg = packageJson
            if (pkg != null) {
                // A new package = a new host (the key): every synced upsert
                // repaints; the screen request outlives it (VAPP-218).
                androidx.compose.runtime.key(pkg, viewing, migrateFrom) {
                    AppPackageSurface(pkg, appIcon, onOpenTeamSettings, appId, teamId, me, grants = grants, readOnly = readOnly, migrateFrom = migrateFrom, screen = screen, onScreen = onScreen)
                }
            } else {
                EmptyState(
                    title = Apps.Copy.PREVIEW_EMPTY,
                    icon = ExpIcons.navApps,
                    size = EmptyStateSize.Page,
                    modifier = Modifier.align(Alignment.Center),
                )
            }
        }
    }
}

/** One package on the app host (`applyTemplate`): its default template
 *  (`Apps.previewTemplate`), or with `screens` (VAPP-218) the resolved
 *  screen's template with `{params}`, the phone tab bar under it (phones
 *  always take `Apps.screenShell`'s bar). #VAPP-251: a package the host
 *  refuses (applied, no surface) = the page-size `copy.needsUpdate` alone;
 *  the validator's issues go to the log. */
@Composable
internal fun AppPackageSurface(
    packageJson: String,
    appIcon: ImageVector,
    onOpenTeamSettings: (teamId: String, section: String) -> Unit,
    appId: String,
    teamId: String?,
    me: String?,
    grants: Boolean,
    readOnly: Boolean,
    migrateFrom: String? = null,
    screen: AppEditorViewModel.ScreenRequest,
    onScreen: (screen: String, params: Map<String, String>) -> Unit,
) {
    val parsed = remember(packageJson) { runCatching { JsonValue.parse(packageJson) }.getOrNull() }
    val packageId = parsed?.get("id")?.string
    // #EXP-1674: the bar paints the navigation's; a `nav: false` screen opens by link.
    val screens = remember(parsed) { parsed?.let(Screens::nav).orEmpty() }
    val resolved = remember(parsed, screen) { parsed?.let { Screens.resolve(it, screen.screen, screen.params) } }
    val shell = Apps.screenShell(screens.size, phone = true)
    val templateId = resolved?.screen ?: parsed?.get("templates")?.obj?.keys?.let { Apps.previewTemplate(it) }
    val consent = remember { AppConsentGate() }
    // #VAPP-230: the upstream reads' answers (the "Connect <server>" banners).
    val upstreams = remember { AppUpstreamAnswers() }
    val latestScreen by rememberUpdatedState(onScreen)
    val navigateToScreen: (NavigateTarget) -> Boolean = remember {
        { target ->
            latestScreen(target.screen, target.params)
            true
        }
    }
    val host = rememberAppHost(
        packages = listOf(packageJson),
        consent = consent,
        appId = appId,
        teamId = teamId,
        me = me,
        themeId = parsed?.get("theme")?.string,
        grants = grants,
        readOnly = readOnly,
        upstreams = upstreams,
        migrateFrom = migrateFrom,
        // VAPP-218: `app_navigate` replaces the request (screen + params).
        navigate = if (screens.isEmpty()) null else navigateToScreen,
    )
    // #FEED-201: the bar's screens page like every phone tab strip (THE
    // pager, `TabPager`): each page opens its own surface on the host; a
    // detail screen (`nav: false`) stands alone.
    val paging = shell == Apps.ScreenShell.Bar && resolved != null && screens.any { it.screen == resolved.screen }
    val pageSurfaces = remember(host) { mutableStateMapOf<String, String>() }
    // A new screen = a fresh surface on a minted id (round 9 §1.4): the id is state.
    var surfaceId by remember(host) { mutableStateOf<String?>(null) }
    var asked by remember(host) { mutableStateOf(false) }
    LaunchedEffect(host, packageId, templateId, resolved, paging) {
        if (paging) return@LaunchedEffect
        if (packageId != null && templateId != null) {
            surfaceId = host.reopenSurface(surfaceId, Apps.Preview.SURFACE_ID, packageId, templateId, resolved?.data)
        }
        asked = true
    }
    val shownSurface = if (paging) pageSurfaces[resolved?.screen] else surfaceId
    val hasSurface = shownSurface?.let(host.surfaces::containsKey) == true
    val shownAsked = if (paging) resolved?.screen in pageSurfaces else asked
    if (Apps.surfaceState(hasPackage = true, applied = shownAsked, hasSurface = hasSurface) == Apps.SurfaceState.NeedsUpdate) {
        LaunchedEffect(packageJson) {
            val issues = runCatching { host.installPackage(packageJson) }.getOrNull()
            Log.w("AppHost", "app $appId: the package paints nothing: $issues")
        }
        Box(Modifier.fillMaxSize()) {
            EmptyState(
                title = Apps.Copy.NEEDS_UPDATE,
                icon = appIcon,
                size = EmptyStateSize.Page,
                modifier = Modifier.align(Alignment.Center),
            )
        }
        return
    }
    val banner: @Composable () -> Unit = {
        AppHostBanner(
            host,
            upstreams = upstreams,
            onConnect = teamId?.let { team -> { onOpenTeamSettings(team, com.exponential.app.domain.SettingsNav.MCP) } },
        )
    }
    Column(Modifier.fillMaxSize()) {
        if (paging && parsed != null && packageId != null) {
            TabPager(
                pages = screens.map { it.screen },
                selected = resolved!!.screen,
                onSelect = { latestScreen(it, emptyMap()) },
                key = { it },
                modifier = Modifier.weight(1f).fillMaxWidth(),
            ) { tab ->
                // The shown screen keeps its request's params; a neighbour opens with its own.
                val page = remember(parsed, tab, resolved) { if (tab == resolved.screen) resolved else Screens.resolve(parsed, tab, null) }
                LaunchedEffect(host, packageId, page) {
                    if (page != null) pageSurfaces[tab] = host.reopenSurface(pageSurfaces[tab], Apps.Preview.SURFACE_ID, packageId, page.screen, page.data)
                }
                AppScreenScroller(banner) { pageSurfaces[tab]?.let { HostSurface(host, it, Modifier.fillMaxWidth()) } }
            }
        } else {
            AppScreenScroller(banner, Modifier.weight(1f)) { surfaceId?.let { HostSurface(host, it, Modifier.fillMaxWidth()) } }
        }
        if (shell == Apps.ScreenShell.Bar) {
            PhoneTabCapsule(
                items = screens.map { s ->
                    PhoneTabItem(
                        key = s.screen,
                        label = s.title,
                        icon = s.icon?.let { ExpIcons.byConcept(it) ?: ExpIcons.byName(it) } ?: ExpIcons.uiIconPlaceholder,
                        testTag = "${Apps.TestIds.PREVIEW_SCREENS}-${s.screen}",
                    )
                },
                selected = resolved?.screen,
                onSelect = { latestScreen(it, emptyMap()) },
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(8.dp)
                    .testTag(Apps.TestIds.PREVIEW_SCREENS),
            )
        }
    }
    AppConsentCard(consent)
}

/** One screen's surface under the host banner, in its own scroller. */
@Composable
private fun AppScreenScroller(banner: @Composable () -> Unit, modifier: Modifier = Modifier, surface: @Composable () -> Unit) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .padding(Apps.Preview.PADDING.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        banner()
        surface()
    }
}

/** VERSIONS (VAPP-177): ONE grouped list, newest first: the `v{n}` pill +
 *  the note over a muted `who · when`; eApps V1 (`Apps.versionRow`): the
 *  published row carries `Published`, the head row ahead of it `Current` +
 *  Publish, every other row a ghost Preview + Publish (write access only). */
@Composable
private fun AppVersionsList(
    versions: List<AppVersionEntity>,
    pointers: Apps.Versions,
    users: Map<String, UserEntity>,
    canWrite: Boolean,
    publishing: Int?,
    onPreview: (Int) -> Unit,
    onPublish: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.testTag(Apps.TestIds.VERSIONS)) {
        if (versions.isEmpty()) {
            EmptyState(title = Apps.Copy.VERSIONS_EMPTY, icon = ExpIcons.uiHistory, size = EmptyStateSize.List)
            return@Column
        }
        val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        Column(Modifier.fillMaxWidth().glassGroup()) {
            versions.forEachIndexed { index, version ->
                if (index > 0) GroupDivider()
                val number = version.number ?: 0
                val author = version.createdBy?.let { users[it] }
                val note = version.note?.trim()?.takeIf { it.isNotEmpty() }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("${Apps.TestIds.VERSION_ROW}-$number")
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                ) {
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        // The pill and the note on ONE line over the caption (web, iOS).
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            GlassPill(Apps.versionLabel(number), size = PillSize.Sm)
                            Text(
                                note ?: Apps.Copy.NO_NOTE,
                                color = if (note == null) muted else MaterialTheme.colorScheme.onSurface,
                                fontWeight = FontWeight.Medium,
                                style = MaterialTheme.typography.bodyMedium,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        Text(
                            listOfNotNull(
                                author?.let { userDisplayName(it, it.id) } ?: Apps.Copy.BY_AGENT,
                                version.createdAt?.let { relativeTime(it) }?.takeIf { it.isNotEmpty() },
                                // eApps R8 (apps.json `migrationCases`): the record sweep's line.
                                Apps.versionMigration(version.migrationState, version.migrationTotal ?: 0, version.migrationDone ?: 0, version.migrationError),
                            ).joinToString(" · "),
                            color = muted,
                            style = MaterialTheme.typography.labelSmall,
                            // The sweep's words sit after the author and time.
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    val kind = Apps.versionRow(pointers, number)
                    if (kind == Apps.VersionRowKind.PUBLISHED) {
                        GlassPill(Apps.Copy.PUBLISHED, size = PillSize.Sm)
                    } else {
                        if (kind == Apps.VersionRowKind.HEAD) {
                            GlassPill(Apps.Copy.HEAD, size = PillSize.Sm)
                        } else {
                            Text(
                                Apps.Copy.PREVIEW,
                                // The phones' ghost button: the 12 sp medium label, full foreground.
                                style = GlassPillDefaults.textStyle(PillSize.Sm),
                                color = MaterialTheme.colorScheme.onSurface,
                                modifier = Modifier
                                    .clip(RoundedCornerShape(GlassTokens.RowRadius))
                                    .clickable { onPreview(number) }
                                    .padding(horizontal = 8.dp, vertical = 6.dp)
                                    .testTag(Apps.TestIds.VERSION_PREVIEW),
                            )
                        }
                        if (canWrite) {
                            GlassPill(
                                Apps.Copy.PUBLISH,
                                size = PillSize.Sm,
                                onClick = { onPublish(number) },
                                enabled = publishing == null,
                                modifier = Modifier.testTag(Apps.TestIds.VERSION_PUBLISH),
                            )
                        }
                    }
                }
            }
        }
    }
}

/** The chat pane: the editor session's own session view (once it exists),
 *  else the empty chat whose composer's first send opens it. */
@Composable
private fun AppChatPane(
    app: AppListEntry,
    rootId: String?,
    /** The editor session's chat ([AppEditorChat]); null = none yet. */
    chat: ChatViewModel?,
    padding: PaddingValues,
    viewModel: AppEditorViewModel,
    onOpenSession: (String) -> Unit,
    onOpenApp: (String) -> Unit = {},
) {
    if (rootId != null && chat != null) {
        AppEditorConversation(
            rootId = rootId,
            padding = padding,
            viewModel = viewModel,
            onOpenSession = onOpenSession,
        )
        return
    }
    val sending by viewModel.sending.collectAsStateWithLifecycle()
    val deviceId by viewModel.deviceId.collectAsStateWithLifecycle()
    // #FEED-204 / #EXP-1729: the "+" (apps.json EDITOR DEVICE, `editorHeader`).
    // This first composer takes text only, so Device, Account and Model are
    // its rows; the picks ride the first send.
    val editorDevice by viewModel.editorDevice.collectAsStateWithLifecycle()
    val runner by viewModel.editorRunner.collectAsStateWithLifecycle()
    var plusOpen by remember { mutableStateOf(false) }
    // VAPP-176: a DRAFT (no version yet) with an empty chat = the starters +
    // the composer open and focused on `draftPlaceholder`.
    val draft = Apps.isDraft(app.headVersion)
    // VAPP-234: the draft's "Start from a template" = the import sheet.
    var importing by rememberSaveable { mutableStateOf(false) }
    if (importing) {
        val apps: AppsViewModel = hiltViewModel()
        val busy by apps.busy.collectAsStateWithLifecycle()
        val failure by apps.failure.collectAsStateWithLifecycle()
        FailedActionToast(failure, onShown = apps::clearFailure)
        AppImportSheet(
            busy = busy,
            onDismiss = { importing = false },
            onValidate = apps::validateImport,
            onImport = { text, upstreamMappings -> apps.importApp(text, upstreamMappings) { id -> importing = false; onOpenApp(id) } },
            onTemplate = { template, includeRecords ->
                apps.importTemplate(template, includeRecords) { id -> importing = false; onOpenApp(id) }
            },
        )
    }
    var text by rememberSaveable { mutableStateOf("") }
    var expanded by rememberSaveable { mutableStateOf(draft) }
    val bottomInset = padding.calculateBottomPadding()
    val placeholder = when {
        deviceId == null -> Apps.Copy.NO_DEVICE
        draft -> Apps.Copy.DRAFT_PLACEHOLDER
        else -> Apps.Copy.COMPOSER_PLACEHOLDER
    }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .consumeWindowInsets(PaddingValues(bottom = bottomInset))
            .imePadding()
            .testTag(Apps.TestIds.CHAT),
    ) {
        if (draft) {
            BoxWithConstraints(Modifier.padding(padding).fillMaxSize()) {
                Box(
                    contentAlignment = Alignment.Center,
                    modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).heightIn(min = maxHeight),
                ) {
                    AppStarters(
                        enabled = deviceId != null && !sending,
                        onStart = { starter -> viewModel.send(starter) {} },
                        onFromTemplate = { importing = true },
                    )
                }
            }
        } else {
            EmptyState(
                title = Apps.Copy.CHAT_EMPTY,
                icon = ExpIcons.navApps,
                size = EmptyStateSize.List,
                modifier = Modifier.padding(padding).padding(horizontal = 24.dp).align(Alignment.Center),
            )
        }
        FloatingBarEdge(
            bottomInset = bottomInset,
            visible = !expanded,
            modifier = Modifier.align(Alignment.BottomCenter).fillMaxWidth().padding(bottom = bottomInset),
        ) {
            FloatingComposer(
                expanded = expanded,
                onExpandedChange = { expanded = it },
                placeholder = placeholder,
                hasDraft = text.isNotBlank(),
                capsuleTestTag = "app-composer-capsule",
                modifier = Modifier.testTag(Apps.TestIds.COMPOSER),
            ) {
                val focus = remember { FocusRequester() }
                var focused by remember { mutableStateOf(false) }
                // One tap on the capsule opens AND focuses (a kept draft that
                // reopens the card by itself takes no keyboard).
                if ((draft || text.isBlank()) && deviceId != null) {
                    LaunchedEffect(Unit) {
                        // requestFocus on an unplaced node no-ops: retry until it lands.
                        var attempts = 0
                        while (attempts < 10 && !focused) {
                            runCatching { focus.requestFocus() }
                            withFrameNanos { }
                            attempts++
                        }
                    }
                }
                GlassComposer(
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                    tools = {
                        ComposerToolButton(
                            ExpIcons.uiAdd,
                            contentDescription = ComposerMenu.PLUS_LABEL,
                            onClick = { plusOpen = true },
                            enabled = deviceId != null,
                            modifier = Modifier.testTag(ComposerMenu.PLUS_TEST_ID),
                        )
                    },
                    submit = {
                        ComposerSubmitButton(
                            ExpIcons.uiSubmit,
                            contentDescription = ChatUiCopy.SEND,
                            onClick = { viewModel.send(text) { text = "" } },
                            enabled = text.isNotBlank() && deviceId != null,
                            sending = sending,
                            modifier = Modifier.testTag("app-composer-submit"),
                        )
                    },
                ) {
                    GlassTextField(
                        value = text,
                        onValueChange = { text = it },
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("app-composer-field")
                            .focusRequester(focus)
                            .onFocusChanged { focused = it.isFocused }
                            .then(fieldModifier),
                        placeholder = placeholder,
                        enabled = deviceId != null,
                        minLines = 2,
                        maxLines = 8,
                        bordered = false,
                    )
                }
            }
        }
    }
    if (plusOpen) {
        val menu = editorDevice.composerDevice(viewModel::pickDevice)
        val account = runner.composerAccount(viewModel::pickAccount)
        val editorRows = setOf(ComposerMenuRowId.Device, ComposerMenuRowId.Account, ComposerMenuRowId.Model)
        ComposerPlusMenuSheet(
            layout = ComposerMenu.composerMenuLayout(ComposerMenuSurface.Session, ComposerMenuConditions(device = true, account = true, model = true))
                .filter { it is ComposerMenuEntry.Row && it.id in editorRows },
            codex = runner.agent == "codex",
            effortValue = null,
            effortEnabled = false,
            ultracode = false,
            mcpValue = null,
            computerUse = false,
            canAttach = false,
            onRow = {},
            onUltracodeChange = {},
            onComputerUseChange = {},
            onDismiss = { plusOpen = false },
            issuePage = {},
            values = mapOf(
                ComposerMenuRowId.Device to menu.label,
                ComposerMenuRowId.Account to runner.account?.email,
                ComposerMenuRowId.Model to runner.model.takeIf { it.isNotEmpty() }?.let(::modelLabel),
            ),
            pages = teamsPagePlus(null, mapOf(
                ComposerMenuRowId.Device to { close ->
                    DevicePicker(
                        devices = menu.devices,
                        value = menu.value,
                        onChange = menu.onPick,
                        open = true,
                        onOpenChange = { open -> if (!open) close() },
                    )
                },
                // #EXP-1729: THE launch composer's account and model pickers.
                ComposerMenuRowId.Account to { close ->
                    AccountPicker(
                        options = account.options,
                        value = account.value,
                        onChange = account.onPick,
                        open = true,
                        onOpenChange = { open -> if (!open) close() },
                    )
                },
                ComposerMenuRowId.Model to { close ->
                    ModelPicker(
                        options = runner.modelOptions.map { ModelPickerOption(it, modelLabel(it)) },
                        value = runner.model,
                        onChange = viewModel::pickModel,
                        open = true,
                        onOpenChange = { open -> if (!open) close() },
                    )
                },
            )),
        )
    }
}

/** #EXP-1729: the Account › row's binding over the editor's runner picks. */
private fun AppEditorViewModel.EditorRunner.composerAccount(onPick: (String) -> Unit) = SessionComposerAccount(
    options = accountOptions.map { it.toPickerAccount() },
    value = account?.key,
    onPick = onPick,
)

/** #FEED-204: the Device › row's binding. */
private fun AppEditorViewModel.EditorDevice.composerDevice(onPick: (String) -> Unit) = SessionComposerDevice(
    devices = rows,
    value = choice.deviceId,
    onPick = onPick,
)

/** VAPP-176: a draft's empty chat = the page-size EmptyState
 *  `starterTitle` whose actions are one pill per starter; a tap SENDS that
 *  text as the first message (disabled like the composer without a device). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AppStarters(enabled: Boolean, onStart: (String) -> Unit, onFromTemplate: () -> Unit) {
    EmptyState(
        title = Apps.Copy.STARTER_TITLE,
        icon = ExpIcons.navApps,
        size = EmptyStateSize.Page,
        action = {
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.padding(top = 4.dp),
            ) {
                Apps.Copy.STARTERS.forEach { starter ->
                    GlassPill(
                        starter,
                        onClick = { onStart(starter) },
                        enabled = enabled,
                        modifier = Modifier.testTag(Apps.TestIds.STARTER),
                    )
                }
                // VAPP-234: never disabled (an import needs no device).
                GlassPill(
                    AppTemplates.Copy.FROM_TEMPLATE,
                    onClick = onFromTemplate,
                    modifier = Modifier.testTag(AppTemplates.TestIds.FROM_TEMPLATE),
                )
            }
        },
    )
}

/**
 * #EXP-1732: the editor session's Chat tab = the ROOT's own session view
 * ([com.exponential.app.ui.session.SessionPage]: its raw work + traces, its
 * cards, THE Dock and THE composer, its Device › row = the editor's pick-and-
 * move, #FEED-204 / #EXP-1738). The root's view model is shared by key with
 * the page.
 */
@Composable
private fun AppEditorConversation(
    rootId: String,
    padding: PaddingValues,
    viewModel: AppEditorViewModel,
    onOpenSession: (String) -> Unit,
) {
    val rootSession = hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(
        key = "session:$rootId",
    ) { factory -> factory.create(rootId) }
    // #FEED-204: an ended root's next send may go to another device.
    val editorDevice by viewModel.editorDevice.collectAsStateWithLifecycle()
    LaunchedEffect(rootSession, editorDevice.appMove) { rootSession.appMove = editorDevice.appMove }
    // #EXP-1729: an ENDED root's Account › / Model › = picks for its next
    // send (they ride the revive); a live root's rows are its own switches.
    val runner by viewModel.editorRunner.collectAsStateWithLifecycle()
    val rootRow by viewModel.root.collectAsStateWithLifecycle()
    val ended = rootRow?.isEnded == true
    LaunchedEffect(rootSession, ended, runner) {
        rootSession.appPicks = if (ended) {
            AgentSessionViewModel.AppPicks(
                account = runner.account?.let { com.exponential.app.domain.AccountOptions.pinnedAccount(it.id) },
                model = runner.model.takeIf { it.isNotEmpty() },
            )
        } else {
            null
        }
    }
    Box(Modifier.fillMaxSize().testTag(Apps.TestIds.CHAT)) {
        com.exponential.app.ui.session.SessionPage(
            sessionId = rootId,
            padding = padding,
            onOpenIssue = {},
            onOpenOrigin = { id, _ -> onOpenSession(id) },
            // #EXP-1738: a pick on a live root also moves it (the session
            // pin's request; a refusal = its toast).
            deviceMenu = editorDevice.composerDevice { picked ->
                viewModel.pickDevice(picked)
                if (viewModel.editing) rootSession.moveToDevice(picked)
            },
            accountMenu = if (ended) runner.composerAccount(viewModel::pickAccount) else null,
            modelMenu = if (ended) SessionComposerModel(runner.model, viewModel::pickModel) else null,
        )
    }
}
