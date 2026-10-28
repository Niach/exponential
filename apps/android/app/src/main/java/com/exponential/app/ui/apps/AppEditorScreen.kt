package com.exponential.app.ui.apps

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import at.exponential.ui.host.HostSurface
import at.exponential.ui.json.JsonValue
import at.exponential.ui.primitives.EmptyStateSize
import com.exponential.app.data.db.AppVersionEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.AppExport
import com.exponential.app.domain.Apps
import com.exponential.app.domain.AskDock
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.domain.userMessageTime
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.ComposerSubmitButton
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.FloatingBarEdge
import com.exponential.app.ui.components.FloatingComposer
import com.exponential.app.ui.components.GlassComposer
import com.exponential.app.ui.components.GlassPill
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
import com.exponential.app.ui.session.SessionComposer
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.thread.ThreadAgent
import com.exponential.app.ui.thread.ThreadAnswer
import com.exponential.app.ui.thread.ThreadDock
import com.exponential.app.ui.thread.ThreadConversation
import com.exponential.app.ui.thread.ThreadPlanDecision
import com.exponential.app.ui.thread.ThreadRowMeta
import com.exponential.app.ui.thread.ThreadUiCopy
import com.exponential.app.ui.thread.ThreadViewModel
import com.exponential.app.ui.thread.clockTime
import com.exponential.app.ui.work.WorkTopBar
import dev.chrisbanes.haze.rememberHazeState

// VAPP-110 (fixtures/apps.json, web `components/apps/app-editor.tsx`): THE
// EDITOR on a phone = the header (glyph + name + the version pill) with the
// Chat | Preview | Versions segmented control under it, and ONE pane. Chat =
// the member's EDITOR SESSION (the agent channel's rows + THE composer; none
// yet = the empty chat whose first send opens it); Preview = the head
// package's first template through the app host; Versions = newest first,
// a row previews that version (Restore = a new head). Leaving the editor ENDS
// the live editor session; the next send continues it.

private enum class EditorTab { Chat, Preview, Versions }

private fun tabLabel(tab: EditorTab) = when (tab) {
    EditorTab.Chat -> Apps.Copy.TAB_CHAT
    EditorTab.Preview -> Apps.Copy.TAB_PREVIEW
    EditorTab.Versions -> Apps.Copy.TAB_VERSIONS
}

@Composable
fun AppEditorScreen(
    onBack: () -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    onOpenSession: (sessionId: String) -> Unit,
    viewModel: AppEditorViewModel = hiltViewModel(),
) {
    val app by viewModel.app.collectAsStateWithLifecycle()
    val root by viewModel.root.collectAsStateWithLifecycle()
    val versions by viewModel.versions.collectAsStateWithLifecycle()
    val users by viewModel.users.collectAsStateWithLifecycle()
    val viewing by viewModel.viewing.collectAsStateWithLifecycle()
    val restoring by viewModel.restoring.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    val me by viewModel.me.collectAsStateWithLifecycle()
    val toastLine by viewModel.toast.collectAsStateWithLifecycle()
    val records by viewModel.records.collectAsStateWithLifecycle()
    var exportOpen by rememberSaveable { mutableStateOf(false) }
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

    var tab by rememberSaveable { mutableStateOf(EditorTab.Chat) }
    val entry = app
    CompositionLocalProvider(LocalDetailHaze provides rememberHazeState()) {
        Scaffold(
            containerColor = Color.Transparent,
            modifier = Modifier.testTag(Apps.TestIds.EDITOR),
            topBar = {
                WorkTopBar(
                    title = entry?.name.orEmpty(),
                    titleContent = {
                        if (entry != null) {
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                AppGlyph(entry.icon, entry.color, size = 20.dp)
                                Text(
                                    entry.name,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.weight(1f, fill = false).testTag("app-editor-title"),
                                )
                                AppVersionPill(entry)
                            }
                        }
                    },
                    onBack = onBack,
                    verb = null,
                    verbEnabled = false,
                    onVerb = {},
                    // VAPP-121: Export = the header's trailing round button
                    // (web phones' `app-editor-export`), once there is a version.
                    menu = if (entry != null && entry.headVersion > 0) {
                        {
                            CircleIconButton(
                                ExpIcons.uiShare,
                                contentDescription = AppExport.Copy.EXPORT,
                                onClick = { exportOpen = true },
                                glyphSize = 16.dp,
                                modifier = Modifier.padding(end = 8.dp).testTag("app-editor-export"),
                            )
                        }
                    } else {
                        null
                    },
                    tabs = {
                        GlassSegmentedControl(
                            options = EditorTab.entries,
                            selected = tab,
                            label = ::tabLabel,
                            onSelect = { tab = it },
                            testTag = { "app-editor-tab-${it.name.lowercase()}" },
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 16.dp, vertical = 8.dp)
                                .testTag(Apps.TestIds.TABS),
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
            when (tab) {
                EditorTab.Chat -> AppChatPane(
                    app = entry,
                    rootId = root?.id,
                    padding = padding,
                    viewModel = viewModel,
                    onOpenSession = onOpenSession,
                )
                EditorTab.Preview -> AppPreviewPane(
                    packageJson = if (viewing != null) viewing?.packageJson else entry.packageJson,
                    viewing = viewing?.number,
                    restoring = restoring,
                    onRestore = viewModel::restore,
                    onHead = viewModel::clearViewing,
                    onOpenIssue = onOpenIssue,
                    appId = viewModel.appId,
                    teamId = entry.teamId,
                    me = me,
                    modifier = Modifier.padding(padding),
                )
                EditorTab.Versions -> AppVersionsList(
                    versions = versions,
                    head = entry.headVersion,
                    viewing = viewing?.number,
                    users = users,
                    onPick = { number ->
                        viewModel.pick(number)
                        tab = EditorTab.Preview
                    },
                    modifier = Modifier
                        .padding(padding)
                        .fillMaxSize()
                        .verticalScroll(rememberScrollState())
                        .padding(16.dp),
                )
            }
        }
    }
    if (exportOpen && entry != null) {
        AppExportSheet(app = entry, records = records, onDismiss = { exportOpen = false })
    }
}

/** THE preview frame: the viewing banner (Restore / Current), then the
 *  package's first template through the app host, or the empty state. */
@Composable
private fun AppPreviewPane(
    packageJson: String?,
    viewing: Int?,
    restoring: Boolean,
    onRestore: () -> Unit,
    onHead: () -> Unit,
    onOpenIssue: (String) -> Unit,
    appId: String,
    teamId: String?,
    me: String?,
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
                GlassPill(Apps.Copy.RESTORE, size = PillSize.Sm, primary = true, onClick = onRestore, enabled = !restoring, modifier = Modifier.testTag("app-restore"))
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
                Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Apps.Preview.PADDING.dp)) {
                    // A new package = a new host (the key): every synced upsert repaints.
                    androidx.compose.runtime.key(pkg, viewing) { AppPackageSurface(pkg, onOpenIssue, appId, teamId, me, grants = viewing == null) }
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

/** One package's first template on the app host (`applyTemplate`). */
@Composable
private fun AppPackageSurface(packageJson: String, onOpenIssue: (String) -> Unit, appId: String, teamId: String?, me: String?, grants: Boolean) {
    val parsed = remember(packageJson) { runCatching { JsonValue.parse(packageJson) }.getOrNull() }
    val packageId = parsed?.get("id")?.string
    val templateId = parsed?.get("templates")?.obj?.keys?.firstOrNull()
    val consent = remember { AppConsentGate() }
    val host = rememberAppHost(
        packages = listOf(packageJson),
        consent = consent,
        onOpenIssue = onOpenIssue,
        appId = appId,
        teamId = teamId,
        me = me,
        themeId = parsed?.get("theme")?.string,
        grants = grants,
    )
    LaunchedEffect(host, packageId, templateId) {
        if (packageId != null && templateId != null) {
            host.receive(
                JsonValue.Obj(
                    mapOf(
                        "version" to JsonValue.Str("v0.9"),
                        "applyTemplate" to JsonValue.Obj(
                            mapOf(
                                "surfaceId" to JsonValue.Str(Apps.Preview.SURFACE_ID),
                                "templateId" to JsonValue.Str(templateId),
                                "packageId" to JsonValue.Str(packageId),
                            ),
                        ),
                    ),
                ),
            )
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        AppHostBanner(host)
        HostSurface(host, Apps.Preview.SURFACE_ID, Modifier.fillMaxWidth())
    }
    AppConsentCard(consent)
}

/** The Versions list: newest first, the head marked, a row previews it. */
@Composable
private fun AppVersionsList(
    versions: List<AppVersionEntity>,
    head: Int,
    viewing: Int?,
    users: Map<String, UserEntity>,
    onPick: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.testTag(Apps.TestIds.VERSIONS)) {
        if (versions.isEmpty()) {
            EmptyState(title = Apps.Copy.NO_VERSION, icon = ExpIcons.uiHistory, size = EmptyStateSize.List)
            return@Column
        }
        val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        versions.forEach { version ->
            val number = version.number ?: 0
            val author = version.createdBy?.let { users[it] }
            val active = if (viewing == null) number == head else viewing == number
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("${Apps.TestIds.VERSION_ROW}-$number")
                    .flatRow(active = active)
                    .clickable { onPick(number) }
                    .padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                Text(
                    Apps.versionLabel(number),
                    fontWeight = FontWeight.Medium,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.width(40.dp),
                )
                Text(
                    version.note?.trim()?.takeIf { it.isNotEmpty() } ?: Apps.Copy.NO_NOTE,
                    color = muted,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                if (number == head) GlassPill(Apps.Copy.HEAD, size = PillSize.Sm)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (author != null) UserAvatar(author, author.name, size = 16.dp)
                    Text(
                        listOfNotNull(
                            author?.let { userDisplayName(it, it.id) } ?: Apps.Copy.BY_AGENT,
                            WireTimestamps.parseEpochMs(version.createdAt)?.let { userMessageTime(it) },
                        ).joinToString(" · "),
                        color = muted,
                        style = MaterialTheme.typography.labelSmall,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.widthIn(max = 140.dp),
                    )
                }
            }
        }
    }
}

/** The chat pane: the editor session's conversation (once it exists), else
 *  the empty chat whose composer's first send opens it. */
@Composable
private fun AppChatPane(
    app: AppListEntry,
    rootId: String?,
    padding: PaddingValues,
    viewModel: AppEditorViewModel,
    onOpenSession: (String) -> Unit,
) {
    if (rootId != null) {
        SessionArgsStore(rootId) {
            AppEditorConversation(app = app, rootId = rootId, padding = padding, onOpenSession = onOpenSession)
        }
        return
    }
    val sending by viewModel.sending.collectAsStateWithLifecycle()
    val deviceId by viewModel.deviceId.collectAsStateWithLifecycle()
    var draft by rememberSaveable { mutableStateOf("") }
    var expanded by rememberSaveable { mutableStateOf(false) }
    val bottomInset = padding.calculateBottomPadding()
    val placeholder = if (deviceId != null) Apps.Copy.COMPOSER_PLACEHOLDER else Apps.Copy.NO_DEVICE
    Box(
        modifier = Modifier
            .fillMaxSize()
            .consumeWindowInsets(PaddingValues(bottom = bottomInset))
            .imePadding()
            .testTag(Apps.TestIds.CHAT),
    ) {
        EmptyState(
            title = Apps.Copy.CHAT_EMPTY,
            icon = ExpIcons.navApps,
            size = EmptyStateSize.List,
            modifier = Modifier.padding(padding).padding(horizontal = 24.dp).align(Alignment.Center),
        )
        FloatingBarEdge(
            bottomInset = bottomInset,
            visible = !expanded,
            modifier = Modifier.align(Alignment.BottomCenter).fillMaxWidth().padding(bottom = bottomInset),
        ) {
            FloatingComposer(
                expanded = expanded,
                onExpandedChange = { expanded = it },
                placeholder = placeholder,
                hasDraft = draft.isNotBlank(),
                capsuleTestTag = "app-composer-capsule",
                modifier = Modifier.testTag(Apps.TestIds.COMPOSER),
            ) {
                GlassComposer(
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                    submit = {
                        ComposerSubmitButton(
                            ExpIcons.uiSubmit,
                            contentDescription = ThreadUiCopy.SEND,
                            onClick = { viewModel.send(draft) { draft = "" } },
                            enabled = draft.isNotBlank() && deviceId != null,
                            sending = sending,
                            modifier = Modifier.testTag("app-composer-submit"),
                        )
                    },
                ) {
                    GlassTextField(
                        value = draft,
                        onValueChange = { draft = it },
                        modifier = Modifier.fillMaxWidth().testTag("app-composer-field").then(fieldModifier),
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
}

/** The editor session's conversation: the agent channel's rows (the app's
 *  glyph + name on the agent's messages) over THE session composer. */
@Composable
private fun AppEditorConversation(
    app: AppListEntry,
    rootId: String,
    padding: PaddingValues,
    onOpenSession: (String) -> Unit,
) {
    val thread = hiltViewModel<ThreadViewModel>(key = "app-thread:$rootId")
    val view by thread.view.collectAsStateWithLifecycle()
    val messages by thread.messages.collectAsStateWithLifecycle()
    val users by thread.users.collectAsStateWithLifecycle()
    val me by thread.currentUserId.collectAsStateWithLifecycle()
    val failure by thread.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = thread::clearFailure)
    val rootSession = hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(
        key = "session:$rootId",
    ) { factory -> factory.create(rootId) }
    LaunchedEffect(rootSession) { rootSession.ensureConnected() }

    val messagesById = remember(messages) { messages.associateBy { it.id } }
    val meta: (SessionTree.ThreadRow.MessageRow) -> ThreadRowMeta? = { row ->
        val user = row.authorId?.let { users[it] }
        ThreadRowMeta(
            authorName = if (row.author == "user") userDisplayName(user, row.authorId) else "",
            time = messagesById[row.id]?.createdAt?.let(::clockTime).orEmpty(),
            user = user,
        )
    }
    val agent = ThreadAgent(name = app.name, icon = app.icon, color = app.color)
    val v = view
    if (v == null) {
        Box(Modifier.padding(padding).fillMaxSize()) {
            SkeletonRows(modifier = Modifier.align(Alignment.TopCenter).padding(16.dp))
        }
        return
    }
    val onAnswer: ThreadAnswer = { id, answer, settled ->
        thread.answer(id, answer.answer, settled, answer.answers, answer.surfaceData)
    }
    val onDecidePlan: ThreadPlanDecision = { id, d, settled ->
        thread.decidePlan(id, d.approved, d.fresh, d.feedback, settled, d.answers, d.surfaceData)
    }
    val openSession by rememberUpdatedState(onOpenSession)
    var composerExpanded by rememberSaveable { mutableStateOf(false) }
    var bandHeightPx by remember { mutableIntStateOf(0) }
    val topInset = padding.calculateTopPadding()
    val bottomInset = padding.calculateBottomPadding()
    val bandClearance = with(LocalDensity.current) { bandHeightPx.toDp() } + bottomInset
    Box(
        modifier = Modifier
            .fillMaxSize()
            .consumeWindowInsets(PaddingValues(bottom = bottomInset))
            .imePadding()
            .testTag(Apps.TestIds.CHAT),
    ) {
        if (v.rows.isEmpty()) {
            EmptyState(
                title = Apps.Copy.CHAT_EMPTY,
                icon = ExpIcons.navApps,
                size = EmptyStateSize.List,
                modifier = Modifier.padding(padding).padding(horizontal = 24.dp).align(Alignment.Center),
            )
        }
        ThreadConversation(
            rows = v.rows,
            rootId = v.rootId,
            meta = meta,
            onOpenCard = { card -> openSession(card.sessionId) },
            onAnswer = onAnswer,
            onDecidePlan = onDecidePlan,
            canUnqueue = { row -> row.authorId != null && row.authorId == me },
            onUnqueue = thread::unqueue,
            agent = agent,
            modifier = Modifier.fillMaxSize().detailHazeSource(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = topInset + 16.dp, bottom = bandClearance + 12.dp),
            jumpBottomInset = bandClearance,
            onOpenSession = { id -> openSession(id) },
            // S-3: a report / another session's line links that session.
            origin = v.origin,
            onOpenXref = { id -> openSession(id) },
        )
        FloatingBarEdge(
            bottomInset = bottomInset,
            visible = !composerExpanded,
            modifier = Modifier.align(Alignment.BottomCenter).fillMaxWidth().padding(bottom = bottomInset),
        ) {
            Box(Modifier.fillMaxWidth().onSizeChanged { if (it.height != bandHeightPx) bandHeightPx = it.height }) {
                SessionComposer(
                    viewModel = rootSession,
                    expanded = composerExpanded,
                    onExpandedChange = { composerExpanded = it },
                    placeholder = Apps.Copy.COMPOSER_PLACEHOLDER,
                    capsuleTestTag = "app-composer-capsule",
                    showQueue = false,
                    modifier = Modifier.testTag(Apps.TestIds.COMPOSER),
                    // N-12: THE Dock: the tree's waiting cards, then the
                    // root's Queue and banners, over the composer.
                    dock = { sessionDock ->
                        ThreadDock(
                            rows = v.rows,
                            sources = sessionDock.sources,
                            sessionSheet = sessionDock.sheet,
                            action = sessionDock.action,
                        )
                    },
                )
            }
        }
    }
}
