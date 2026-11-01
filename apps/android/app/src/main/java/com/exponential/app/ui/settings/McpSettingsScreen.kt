package com.exponential.app.ui.settings

import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import coil3.compose.AsyncImage
import com.exponential.app.data.api.AppAccessApp
import com.exponential.app.data.api.AppAccessList
import com.exponential.app.data.api.McpActivityList
import com.exponential.app.data.api.McpClientRow
import com.exponential.app.data.api.McpClientsList
import com.exponential.app.data.api.McpServerListRow
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.domain.McpSettings
import com.exponential.app.domain.Prompts
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.ExponentialMark
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.MemberAvatar
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.components.SearchListInput
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.TopBarTitle
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.compactRelativeTime
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup

// #VAPP-185 (fixtures/settings-mcp.json): Settings › MCP, ONE page ×4 (web
// `components/mcp/mcp-settings-page.tsx`): Endpoint · Servers · Clients
// (Mine | Everyone, owners) · Access (+ Activity) · Built-in Exponential
// tools, each a section label over one glass group. Phones never add or
// edit a server: the rows + the member's own Connect (the web page, in the
// browser) / Disconnect.

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun McpPageScaffold(title: String, onBack: () -> Unit, testTag: String, content: @Composable ColumnScope.() -> Unit) {
    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { TopBarTitle(title) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(16.dp)
                .testTag(testTag),
            verticalArrangement = Arrangement.spacedBy(24.dp),
            content = content,
        )
    }
}

@Composable
fun McpSettingsScreen(
    onBack: () -> Unit,
    onOpenActivity: () -> Unit,
    viewModel: McpSettingsViewModel = hiltViewModel(),
) {
    val isOwner by viewModel.isOwner.collectAsStateWithLifecycle()
    val endpoint by viewModel.endpointUrl.collectAsStateWithLifecycle()
    val servers by viewModel.serverRows.collectAsStateWithLifecycle()
    val scope by viewModel.scope.collectAsStateWithLifecycle()
    val clients by viewModel.clients.collectAsStateWithLifecycle()
    val access by viewModel.access.collectAsStateWithLifecycle()
    val pending by viewModel.pending.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    LaunchedEffect(Unit) { viewModel.load() }
    val context = LocalContext.current

    McpSettingsPage(
        onBack = onBack,
        endpoint = endpoint,
        servers = servers,
        pending = pending,
        isOwner = isOwner,
        scope = scope,
        clients = clients,
        access = access,
        onConnect = { server ->
            viewModel.connectUrl(server)?.let { CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(it)) }
        },
        onDisconnect = viewModel::disconnect,
        onScope = viewModel::setScope,
        onRevoke = viewModel::revoke,
        onLevel = viewModel::setLevel,
        onOpenActivity = onOpenActivity,
    )
}

/** The stateless page: null data = still loading (skeleton rows). */
@Composable
internal fun McpSettingsPage(
    onBack: () -> Unit,
    endpoint: String?,
    servers: List<McpServerListRow>?,
    pending: String?,
    isOwner: Boolean,
    scope: String,
    clients: McpClientsList?,
    access: AppAccessList?,
    onConnect: (McpServerListRow) -> Unit,
    onDisconnect: (McpServerListRow) -> Unit,
    onScope: (String) -> Unit,
    onRevoke: (McpClientRow) -> Unit,
    onLevel: (appId: String, userId: String, level: String) -> Unit,
    onOpenActivity: () -> Unit,
) {
    McpPageScaffold(McpSettings.TITLE, onBack, McpSettings.TestIds.PAGE) {
        endpoint?.let { EndpointSection(it) }
        ServersSection(servers = servers, pending = pending, onConnect = onConnect, onDisconnect = onDisconnect)
        ClientsSection(isOwner = isOwner, scope = scope, onScope = onScope, data = clients, onRevoke = onRevoke)
        access?.takeIf { it.apps.isNotEmpty() }?.let { data ->
            AccessSection(data, onLevel = onLevel, onOpenActivity = onOpenActivity)
        }
        BuiltinToolsSection()
    }
}

/** ACTIVITY (`activityPage`): the `mcp_calls` rows newest first. */
@Composable
fun McpActivityScreen(
    onBack: () -> Unit,
    viewModel: McpSettingsViewModel = hiltViewModel(),
) {
    val data by viewModel.activity.collectAsStateWithLifecycle()
    LaunchedEffect(Unit) { viewModel.loadActivity() }
    McpActivityPage(onBack, data)
}

@Composable
internal fun McpActivityPage(onBack: () -> Unit, data: McpActivityList?) {
    McpPageScaffold(McpSettings.ACTIVITY, onBack, McpSettings.TestIds.ACTIVITY) {
        McpActivityList(data)
    }
}

@Composable
internal fun McpActivityList(data: McpActivityList?) {
    Column {
        SectionHeader(McpSettings.ACTIVITY)
        when {
            data == null -> SkeletonRows(modifier = Modifier.padding(vertical = 4.dp))
            data.calls.isEmpty() -> EmptyState(EmptyStateCopy.mcpActivity)
            else -> McpGroup {
                data.calls.forEachIndexed { i, call ->
                    if (i > 0) GroupDivider()
                    McpRow(Modifier.testTag("mcp-activity-row")) {
                        RowText(
                            title = call.tool,
                            caption = McpSettings.activityCaption(
                                owner = data.owner,
                                appName = call.appName,
                                userName = call.userName.orEmpty(),
                                clientName = call.clientName,
                                `when` = compactRelativeTime(call.createdAt),
                            ),
                            mono = true,
                        )
                        GlassPill(McpSettings.statusLabel(call.status), size = PillSize.Sm)
                    }
                }
            }
        }
    }
}

// ── Shared row chrome ────────────────────────────────────────────────────────

/** One glass group of rows (web `SETTINGS_LIST_CLASS`). */
@Composable
private fun McpGroup(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(modifier = modifier.fillMaxWidth().glassGroup(), content = content)
}

/** One row of a group (web `ListRow className="gap-3 px-3 py-2"`). */
@Composable
private fun McpRow(modifier: Modifier = Modifier, onClick: (() -> Unit)? = null, content: @Composable RowScope.() -> Unit) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        content = content,
    )
}

/** The name over its muted caption, taking the row's rest. */
@Composable
private fun RowScope.RowText(
    title: String,
    caption: String?,
    mono: Boolean = false,
    titleTrailing: (@Composable () -> Unit)? = null,
) {
    Column(Modifier.weight(1f)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(
                title,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = if (mono) FontWeight.Normal else FontWeight.Medium,
                fontFamily = if (mono) FontFamily.Monospace else null,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            titleTrailing?.invoke()
        }
        if (!caption.isNullOrEmpty()) {
            Text(
                caption,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun RowGlyph(icon: androidx.compose.ui.graphics.vector.ImageVector, tint: Color? = null) {
    Icon(
        icon,
        contentDescription = null,
        modifier = Modifier.size(16.dp),
        tint = tint ?: MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
    )
}

/** The endpoint on a phone keeps its head and its path (truncated in the middle). */
internal fun middleTruncate(s: String, max: Int = 30): String {
    if (s.length <= max) return s
    val tail = McpSettings.ENDPOINT_PATH.length.coerceAtMost(max / 2)
    return s.take(max - tail - 1) + "…" + s.takeLast(tail)
}

// ── (1) Endpoint ─────────────────────────────────────────────────────────────

@Composable
private fun EndpointSection(url: String) {
    val clipboard = LocalClipboardManager.current
    val toaster = LocalToaster.current
    Column(Modifier.testTag(McpSettings.TestIds.ENDPOINT)) {
        SectionHeader(McpSettings.LABELS.getValue("endpoint"))
        McpGroup {
            McpRow {
                Text(
                    middleTruncate(url),
                    style = MaterialTheme.typography.bodyMedium,
                    fontFamily = FontFamily.Monospace,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                GlassPill(
                    McpSettings.COPY,
                    size = PillSize.Sm,
                    onClick = {
                        clipboard.setText(AnnotatedString(url))
                        toaster.success(McpSettings.COPIED)
                    },
                    modifier = Modifier.testTag("mcp-endpoint-copy"),
                )
            }
        }
    }
}

// ── (2) Servers ──────────────────────────────────────────────────────────────

/** web `mcpServerTarget`: an http server's host, a stdio server's command line. */
internal fun mcpServerTarget(server: McpServerListRow): String =
    if (server.transport == "http") {
        server.url?.let { runCatching { Uri.parse(it).host }.getOrNull() ?: it }.orEmpty()
    } else {
        (listOfNotNull(server.command) + server.args.orEmpty()).filter { it.isNotEmpty() }.joinToString(" ")
    }

@Composable
private fun ServersSection(
    servers: List<McpServerListRow>?,
    pending: String?,
    onConnect: (McpServerListRow) -> Unit,
    onDisconnect: (McpServerListRow) -> Unit,
) {
    Column(Modifier.testTag(McpSettings.TestIds.SERVERS)) {
        SectionHeader(McpSettings.LABELS.getValue("servers"))
        when {
            servers == null -> SkeletonRows(modifier = Modifier.padding(vertical = 4.dp))
            servers.isEmpty() -> EmptyState(EmptyStateCopy.mcpServers)
            else -> McpGroup {
                servers.forEachIndexed { i, server ->
                    if (i > 0) GroupDivider()
                    ServerRow(server, pending == server.id, onConnect = { onConnect(server) }, onDisconnect = { onDisconnect(server) })
                }
            }
        }
    }
}

@Composable
private fun ServerRow(server: McpServerListRow, pending: Boolean, onConnect: () -> Unit, onDisconnect: () -> Unit) {
    val target = mcpServerTarget(server)
    val caption = listOfNotNull(target.takeIf { it.isNotEmpty() }, McpSettings.usedByCaption(server.usedBy)).joinToString(" · ")
    McpRow(Modifier.testTag("mcp-server-row")) {
        RowGlyph(if (server.command != null && server.url == null) ExpIcons.sessionShell else ExpIcons.uiMcp)
        RowText(
            title = server.name.ifBlank { server.id },
            caption = caption,
            titleTrailing = if (server.enabledByDefault) {
                { GlassPill("Default", size = PillSize.Sm) }
            } else {
                null
            },
        )
        val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        when (server.connection.status) {
            "not_needed" -> Text("No sign-in needed", style = MaterialTheme.typography.bodySmall, color = muted)
            "connected" -> GlassPill(
                "Disconnect",
                size = PillSize.Sm,
                enabled = !pending,
                onClick = onDisconnect,
                modifier = Modifier.testTag("mcp-server-disconnect"),
            )
            "expired", "error" -> GlassPill(
                "Reconnect",
                size = PillSize.Sm,
                icon = ExpIcons.uiWarning,
                enabled = !pending,
                onClick = onConnect,
            )
            else -> GlassPill(
                if (server.auth == "secret") "Set key" else "Connect",
                size = PillSize.Sm,
                primary = true,
                enabled = !pending,
                onClick = onConnect,
                modifier = Modifier.testTag("mcp-server-connect"),
            )
        }
    }
}

// ── (3) Clients ──────────────────────────────────────────────────────────────

@Composable
private fun ClientsSection(
    isOwner: Boolean,
    scope: String,
    onScope: (String) -> Unit,
    data: McpClientsList?,
    onRevoke: (McpClientRow) -> Unit,
) {
    var target by remember { mutableStateOf<McpClientRow?>(null) }
    Column(Modifier.testTag(McpSettings.TestIds.CLIENTS)) {
        SectionHeader(McpSettings.LABELS.getValue("clients"))
        if (isOwner) {
            GlassSegmentedControl(
                options = McpSettings.SCOPES.map { it.value },
                selected = scope,
                label = { v -> McpSettings.SCOPES.first { it.value == v }.label },
                onSelect = onScope,
                testTag = { "mcp-clients-scope-$it" },
            )
            Spacer(Modifier.height(8.dp))
        }
        when {
            data == null -> SkeletonRows(modifier = Modifier.padding(vertical = 4.dp))
            data.clients.isEmpty() -> EmptyState(EmptyStateCopy.mcpClients)
            else -> McpGroup {
                data.clients.forEachIndexed { i, client ->
                    if (i > 0) GroupDivider()
                    McpRow(Modifier.testTag("mcp-client-row")) {
                        if (client.icon != null) {
                            AsyncImage(model = client.icon, contentDescription = null, modifier = Modifier.size(16.dp))
                        } else {
                            RowGlyph(ExpIcons.settingsMcp)
                        }
                        RowText(
                            title = client.name,
                            caption = McpSettings.clientCaption(
                                scope = scope,
                                userName = client.userName ?: client.userEmail.orEmpty(),
                                kinds = client.kinds,
                                appsAccess = client.appsAccess,
                                lastUsed = client.lastUsedAt?.let { compactRelativeTime(it) },
                            ),
                        )
                        GlassPill(McpSettings.REVOKE, size = PillSize.Sm, onClick = { target = client })
                    }
                }
            }
        }
    }
    target?.let { client ->
        val prompt = Prompts.RevokeMcpClient.prompt(client.name)
        PromptAlert(
            prompt = prompt,
            onDismiss = { target = null },
            handlers = mapOf(
                "revoke" to {
                    target = null
                    onRevoke(client)
                },
            ),
        )
    }
}

// ── (4) Access ───────────────────────────────────────────────────────────────

@Composable
private fun AppGlyph(app: AppAccessApp) {
    val icon = app.icon?.let { ExpIcons.byName(it) } ?: ExpIcons.navApps
    val tint = app.color?.let { remember(it) { parseColor(it) } }
    RowGlyph(icon, tint)
}

@Composable
private fun AccessSection(data: AppAccessList, onLevel: (appId: String, userId: String, level: String) -> Unit, onOpenActivity: () -> Unit) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(Modifier.testTag(McpSettings.TestIds.ACCESS)) {
        SectionHeader(McpSettings.LABELS.getValue("access"))
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            data.apps.forEach { app ->
                McpGroup(Modifier.testTag("mcp-access-app-${app.slug}")) {
                    McpRow {
                        AppGlyph(app)
                        RowText(title = app.name, caption = null)
                        Text(McpSettings.toolCountLabel(app.toolCount), style = MaterialTheme.typography.bodySmall, color = muted)
                    }
                    data.rows.forEach { member ->
                        GroupDivider()
                        val name = member.name ?: member.email.orEmpty()
                        val level = member.levels[app.id] ?: "write"
                        if (data.canEdit) {
                            Picker(
                                items = McpSettings.LEVELS.map { PickerItem(it.value, it.label) },
                                mode = PickerMode.Single,
                                value = setOf(level),
                                onChange = { picked -> picked.firstOrNull()?.let { onLevel(app.id, member.userId, it) } },
                                title = name,
                            ) { open ->
                                AccessMemberRow(name, member.image, member.userId, level, onClick = open, testTag = "mcp-access-${app.slug}-${member.userId}")
                            }
                        } else {
                            AccessMemberRow(name, member.image, member.userId, level, onClick = null, testTag = "mcp-access-${app.slug}-${member.userId}")
                        }
                    }
                }
            }
            McpGroup {
                McpRow(Modifier.testTag(McpSettings.TestIds.ACTIVITY), onClick = onOpenActivity) {
                    Text(McpSettings.ACTIVITY, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
                    RowGlyph(ExpIcons.uiChevronRight)
                }
            }
        }
    }
}

@Composable
private fun AccessMemberRow(name: String, image: String?, userId: String, level: String, onClick: (() -> Unit)?, testTag: String) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    McpRow(Modifier.testTag(testTag), onClick = onClick) {
        MemberAvatar(imageUrl = image, nameOrEmail = name, userId = userId, size = 20.dp)
        Text(name, style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        Text(McpSettings.levelLabel(level), style = MaterialTheme.typography.bodyMedium, color = if (onClick != null) MaterialTheme.colorScheme.onSurface else muted)
        if (onClick != null) RowGlyph(ExpIcons.uiChevronDown)
    }
}

// ── (5) Built-in Exponential tools ───────────────────────────────────────────

@Composable
private fun BuiltinToolsSection() {
    val tools = remember { McpSettings.builtinTools() }
    var query by rememberSaveable { mutableStateOf("") }
    val shown = remember(query) { McpSettings.filterBuiltinTools(tools, query) }
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(Modifier.testTag(McpSettings.TestIds.BUILTIN)) {
        SectionHeader(McpSettings.LABELS.getValue("builtin"))
        McpGroup(Modifier.padding(bottom = 8.dp)) {
            SearchListInput(
                value = query,
                onValueChange = { query = it },
                placeholder = McpSettings.BUILTIN_FILTER_PLACEHOLDER,
                modifier = Modifier.testTag("mcp-builtin-tools-filter"),
            )
        }
        if (shown.isEmpty()) {
            EmptyState(EmptyStateCopy.builtinToolsNoMatch)
        } else {
            McpGroup {
                shown.forEachIndexed { i, tool ->
                    if (i > 0) GroupDivider()
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Box(Modifier.size(12.dp)) { ExponentialMark(size = 12.dp) }
                        Text(tool.title, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium, maxLines = 1)
                        Text(
                            tool.blurb,
                            style = MaterialTheme.typography.bodySmall,
                            color = muted,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        }
    }
}
