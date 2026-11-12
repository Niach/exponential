package com.exponential.app.ui.settings

import android.net.Uri
import com.exponential.app.ui.theme.ExpMono
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
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import at.exponential.ui.primitives.DisclosureHeader
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
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.SubmitSize
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
// `components/mcp/mcp-settings-page.tsx`): the Overview line · Servers (ONE
// group: the built-ins Exponential + Team apps, then the team's servers;
// every row a DisclosureHeader opening its tools) · Access (+ Activity) ·
// Clients (Mine | Everyone, owners), each a section label over one glass
// group. Phones never add or edit a server: the rows + the member's own
// Connect (the web page, in the browser) / Disconnect.

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
    val instance by viewModel.instanceUrl.collectAsStateWithLifecycle()
    val servers by viewModel.serverRows.collectAsStateWithLifecycle()
    val serverTools by viewModel.serverTools.collectAsStateWithLifecycle()
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
        instance = instance,
        servers = servers,
        serverTools = serverTools,
        pending = pending,
        isOwner = isOwner,
        scope = scope,
        clients = clients,
        access = access,
        onConnect = { server ->
            viewModel.connectUrl(server)?.let { CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(it)) }
        },
        onDisconnect = viewModel::disconnect,
        onLoadTools = viewModel::loadTools,
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
    instance: String?,
    servers: List<McpServerListRow>?,
    serverTools: Map<String, McpToolsLoad>,
    pending: String?,
    isOwner: Boolean,
    scope: String,
    clients: McpClientsList?,
    access: AppAccessList?,
    onConnect: (McpServerListRow) -> Unit,
    onDisconnect: (McpServerListRow) -> Unit,
    onLoadTools: (serverId: String, force: Boolean) -> Unit,
    onScope: (String) -> Unit,
    onRevoke: (McpClientRow) -> Unit,
    onLevel: (appId: String, userId: String, level: String) -> Unit,
    onOpenActivity: () -> Unit,
    /** The rows open at first (closed by default; the captures open some). */
    initiallyOpen: List<String> = emptyList(),
) {
    McpPageScaffold(McpSettings.TITLE, onBack, McpSettings.TestIds.PAGE) {
        Text(
            McpSettings.OVERVIEW,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.testTag(McpSettings.TestIds.OVERVIEW),
        )
        ServersSection(
            instance = instance,
            servers = servers,
            serverTools = serverTools,
            apps = access?.apps.orEmpty(),
            pending = pending,
            initiallyOpen = initiallyOpen,
            onConnect = onConnect,
            onDisconnect = onDisconnect,
            onLoadTools = onLoadTools,
        )
        access?.takeIf { it.apps.isNotEmpty() }?.let { data ->
            AccessSection(data, onLevel = onLevel, onOpenActivity = onOpenActivity)
        }
        ClientsSection(isOwner = isOwner, scope = scope, onScope = onScope, data = clients, pending = pending, onRevoke = onRevoke)
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
            data == null -> SkeletonRows(modifier = Modifier.padding(vertical = 4.dp), rowHeight = MCP_ROW_HEIGHT)
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
/** #EXP-1788: a skeleton row = the real [McpRow]'s height, measured on the
 *  emulator: a server row (its Default pill / Connect button) 55 dp, a
 *  client or activity row (name over caption) 50 dp. */
private val MCP_SERVER_ROW_HEIGHT = 55.dp
private val MCP_ROW_HEIGHT = 50.dp

/** #EXP-1788: a member without a name reads as their email (web + IDE). */
internal fun memberLabel(name: String?, email: String?): String =
    name?.takeIf { it.isNotBlank() } ?: email.orEmpty()

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
                fontFamily = if (mono) ExpMono else null,
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

/** A built-in's URL on a phone keeps its head and its path (truncated in
 *  the middle); [tailLength] = the kept end (default: the `/api/mcp/apps`
 *  path, which the app sheet's hook URLs and secrets keep too). */
internal fun middleTruncate(s: String, max: Int = 30, tailLength: Int = 13): String {
    if (s.length <= max) return s
    val keep = tailLength.coerceAtMost(max / 2)
    return s.take(max - keep - 1) + "…" + s.takeLast(keep)
}

// ── (2) Servers ──────────────────────────────────────────────────────────────

/** web `mcpServerTarget`: an http server's host, a stdio server's command line. */
internal fun mcpServerTarget(server: McpServerListRow): String =
    if (server.transport == "http") {
        server.url?.let { runCatching { Uri.parse(it).host }.getOrNull() ?: it }.orEmpty()
    } else {
        (listOfNotNull(server.command) + server.args.orEmpty()).filter { it.isNotEmpty() }.joinToString(" ")
    }

/** The open-row key of a team server (built-ins use `builtin:<id>`). */
private fun serverKey(server: McpServerListRow) = "server:${server.id}"

@Composable
private fun ServersSection(
    instance: String?,
    servers: List<McpServerListRow>?,
    serverTools: Map<String, McpToolsLoad>,
    apps: List<AppAccessApp>,
    pending: String?,
    initiallyOpen: List<String>,
    onConnect: (McpServerListRow) -> Unit,
    onDisconnect: (McpServerListRow) -> Unit,
    onLoadTools: (serverId: String, force: Boolean) -> Unit,
) {
    // Several rows may be open at once; every row starts closed.
    var open by rememberSaveable { mutableStateOf(initiallyOpen) }
    fun toggle(key: String) {
        open = if (key in open) open - key else open + key
    }
    val toolApps = apps.filter { it.toolCount > 0 || it.tools.isNotEmpty() }
    val order = McpSettings.serverOrder(toolApps.isNotEmpty(), servers.orEmpty())
    Column(Modifier.testTag(McpSettings.TestIds.SERVERS)) {
        SectionHeader(McpSettings.LABELS.getValue("servers"))
        McpGroup {
            order.forEachIndexed { i, entry ->
                if (i > 0) GroupDivider()
                when (entry) {
                    "builtin:exponential" -> ExponentialServerRow(
                        url = instance?.let { McpSettings.builtinEndpoint(it, McpSettings.BUILTIN_EXPONENTIAL) },
                        expanded = "builtin:exponential" in open,
                        onToggle = { toggle("builtin:exponential") },
                    )
                    "builtin:apps" -> AppsServerRow(
                        url = instance?.let { McpSettings.builtinEndpoint(it, McpSettings.BUILTIN_APPS) },
                        apps = toolApps,
                        expanded = "builtin:apps" in open,
                        onToggle = { toggle("builtin:apps") },
                    )
                    is McpServerListRow -> TeamServerRow(
                        server = entry,
                        pending = pending == entry.id,
                        tools = serverTools[entry.id],
                        expanded = serverKey(entry) in open,
                        onToggle = { toggle(serverKey(entry)) },
                        onConnect = { onConnect(entry) },
                        onDisconnect = { onDisconnect(entry) },
                        onLoadTools = { force -> onLoadTools(entry.id, force) },
                    )
                }
            }
            if (servers == null) {
                GroupDivider()
                SkeletonRows(modifier = Modifier.padding(vertical = 4.dp), rowHeight = MCP_SERVER_ROW_HEIGHT)
            }
        }
    }
}

/**
 * One expandable server row: its leading part (chevron, glyph, name, pill,
 * caption) is THE SDK [DisclosureHeader], [actions] stay outside it; open =
 * [tools] under it, in the same group.
 */
@Composable
private fun ExpandableServerRow(
    name: String,
    caption: String?,
    expanded: Boolean,
    onToggle: () -> Unit,
    glyph: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    captionMono: Boolean = false,
    pill: String? = null,
    actions: @Composable RowScope.() -> Unit = {},
    tools: @Composable ColumnScope.() -> Unit,
) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Column(
                Modifier
                    .weight(1f)
                    .clickable(interactionSource = null, indication = null, onClick = onToggle),
            ) {
                DisclosureHeader(
                    title = name,
                    expanded = expanded,
                    onToggle = onToggle,
                    titleStyle = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Medium),
                    height = Dp.Unspecified,
                    leading = { Box(Modifier.size(16.dp), contentAlignment = Alignment.Center) { glyph() } },
                    trailing = pill?.let { { GlassPill(it, size = PillSize.Sm) } },
                )
                if (!caption.isNullOrEmpty()) {
                    Text(
                        caption,
                        style = MaterialTheme.typography.bodySmall,
                        fontFamily = if (captionMono) ExpMono else null,
                        color = muted,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(start = TOOL_INDENT),
                    )
                }
            }
            actions()
        }
        if (expanded) {
            GroupDivider()
            Column(Modifier.fillMaxWidth().padding(bottom = 6.dp), content = tools)
        }
    }
}

/** The chevron (12) + gap (8) + glyph (16) + gap (8): tools line up under the name. */
private val TOOL_INDENT = 44.dp

/** A built-in row's ONE action: Copy its URL (+ the `copied` toast). */
@Composable
private fun CopyUrlPill(url: String, testTag: String) {
    val clipboard = LocalClipboardManager.current
    val toaster = LocalToaster.current
    GlassPill(
        McpSettings.COPY,
        size = PillSize.Sm,
        onClick = {
            clipboard.setText(AnnotatedString(url))
            toaster.success(McpSettings.COPIED)
        },
        modifier = Modifier.testTag(testTag),
    )
}

@Composable
private fun ExponentialServerRow(url: String?, expanded: Boolean, onToggle: () -> Unit) {
    ExpandableServerRow(
        name = McpSettings.BUILTIN_EXPONENTIAL.name,
        caption = url?.let { middleTruncate(it, tailLength = McpSettings.BUILTIN_EXPONENTIAL.path.length) },
        captionMono = true,
        pill = McpSettings.BUILTIN_PILL,
        expanded = expanded,
        onToggle = onToggle,
        glyph = { ExponentialMark(size = 16.dp) },
        modifier = Modifier.testTag(McpSettings.TestIds.BUILTIN),
        actions = { url?.let { CopyUrlPill(it, "mcp-builtin-copy") } },
    ) { BuiltinTools() }
}

@Composable
private fun AppsServerRow(url: String?, apps: List<AppAccessApp>, expanded: Boolean, onToggle: () -> Unit) {
    ExpandableServerRow(
        name = McpSettings.BUILTIN_APPS.name,
        caption = url?.let { middleTruncate(it, tailLength = McpSettings.BUILTIN_APPS.path.length) },
        captionMono = true,
        pill = McpSettings.BUILTIN_PILL,
        expanded = expanded,
        onToggle = onToggle,
        glyph = { RowGlyph(ExpIcons.navApps) },
        modifier = Modifier.testTag(McpSettings.TestIds.APPS_SERVER),
        actions = { url?.let { CopyUrlPill(it, "mcp-apps-copy") } },
    ) {
        // One sub-label per app (alphabetical), its tools in package order.
        apps.sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.name }).forEach { app ->
            ToolSubLabel(app.name)
            app.tools.forEach { ToolRow(it.title.ifBlank { it.name }, it.blurb) }
        }
    }
}

@Composable
private fun TeamServerRow(
    server: McpServerListRow,
    pending: Boolean,
    tools: McpToolsLoad?,
    expanded: Boolean,
    onToggle: () -> Unit,
    onConnect: () -> Unit,
    onDisconnect: () -> Unit,
    onLoadTools: (force: Boolean) -> Unit,
) {
    val target = mcpServerTarget(server)
    val caption = listOfNotNull(
        target.takeIf { it.isNotEmpty() },
        McpSettings.runsOnCaption(server.device?.label),
        McpSettings.usedByCaption(server.usedBy),
    ).joinToString(" · ")
    val state = McpSettings.toolsState(server.transport, server.connection.status)
    ExpandableServerRow(
        name = server.name.ifBlank { server.id },
        caption = caption,
        pill = if (server.enabledByDefault) "Default" else null,
        expanded = expanded,
        onToggle = onToggle,
        glyph = { RowGlyph(if (server.command != null && server.url == null) ExpIcons.sessionShell else ExpIcons.uiMcp) },
        modifier = Modifier.testTag("mcp-server-row"),
        actions = { ServerAction(server, pending, onConnect, onDisconnect) },
    ) {
        Column(Modifier.testTag(McpSettings.TestIds.SERVER_TOOLS)) {
            when (state) {
                McpSettings.ToolsState.Connect -> ToolsLine(McpSettings.TOOLS_CONNECT)
                McpSettings.ToolsState.Stdio -> ToolsLine(McpSettings.TOOLS_STDIO)
                McpSettings.ToolsState.List -> {
                    LaunchedEffect(server.id) { onLoadTools(false) }
                    when (tools) {
                        null, McpToolsLoad.Loading -> SkeletonRows(
                            modifier = Modifier.padding(start = TOOL_INDENT, end = 12.dp, top = 4.dp),
                            rows = 3,
                            rowHeight = TOOL_ROW_HEIGHT,
                            dot = false,
                        )
                        is McpToolsLoad.Failed -> Row(
                            modifier = Modifier.fillMaxWidth().padding(start = TOOL_INDENT, end = 12.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            Text(
                                McpSettings.toolsFailedLine(tools.reason),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                                modifier = Modifier.weight(1f).padding(vertical = 8.dp),
                            )
                            TextButton(onClick = { onLoadTools(true) }, modifier = Modifier.testTag("mcp-server-tools-retry")) {
                                Text(McpSettings.RETRY, style = MaterialTheme.typography.bodySmall)
                            }
                        }
                        is McpToolsLoad.Loaded ->
                            if (tools.tools.isEmpty()) {
                                ToolsLine(McpSettings.TOOLS_NONE)
                            } else {
                                tools.tools.forEach { ToolRow(it.title.ifBlank { it.name }, it.blurb) }
                            }
                    }
                }
            }
        }
    }
}

/** The viewer's ONE action on a team server row. */
@Composable
private fun ServerAction(server: McpServerListRow, pending: Boolean, onConnect: () -> Unit, onDisconnect: () -> Unit) {
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
        // Web's outline `Button size="sm"`, its warning glyph in yellow.
        "expired", "error" -> GlassSubmitButton(
            "Reconnect",
            size = SubmitSize.Small,
            outline = true,
            icon = {
                Icon(
                    ExpIcons.uiWarning,
                    contentDescription = null,
                    tint = Color(0xFFFACC15),
                    modifier = Modifier.size(16.dp),
                )
            },
            enabled = !pending,
            onClick = onConnect,
        )
        // EXP-1676: web's `Button size="sm"`: the glyph, the text button's shape.
        else -> GlassSubmitButton(
            if (server.auth == "secret") "Set key" else "Connect",
            size = SubmitSize.Small,
            icon = {
                Icon(
                    if (server.auth == "secret") ExpIcons.uiPrivate else ExpIcons.uiSignIn,
                    contentDescription = null,
                    modifier = Modifier.size(16.dp),
                )
            },
            enabled = !pending,
            onClick = onConnect,
            modifier = Modifier.testTag("mcp-server-connect"),
        )
    }
}

private val TOOL_ROW_HEIGHT = 20.dp

/** A muted line in place of an open row's tools. */
@Composable
private fun ToolsLine(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        modifier = Modifier.fillMaxWidth().padding(start = TOOL_INDENT, end = 12.dp, top = 8.dp, bottom = 2.dp),
    )
}

/** A sub-label over one category of an open row's tools. */
@Composable
private fun ToolSubLabel(label: String) {
    Text(
        label,
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.fillMaxWidth().padding(start = TOOL_INDENT, end = 12.dp, top = 10.dp, bottom = 2.dp),
    )
}

/** One tool of an open row: the title in medium + its muted blurb. */
@Composable
private fun ToolRow(title: String, blurb: String, mark: Boolean = false) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = if (mark) TOOL_INDENT - 20.dp else TOOL_INDENT, end = 12.dp, top = 4.dp, bottom = 4.dp)
            .testTag("mcp-tool-row"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (mark) Box(Modifier.size(12.dp)) { ExponentialMark(size = 12.dp) }
        Text(title, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium, maxLines = 1)
        Text(
            blurb,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
    }
}

// ── (4) Clients ──────────────────────────────────────────────────────────────

@Composable
private fun ClientsSection(
    isOwner: Boolean,
    scope: String,
    onScope: (String) -> Unit,
    data: McpClientsList?,
    pending: String?,
    onRevoke: (McpClientRow) -> Unit,
) {
    var target by remember { mutableStateOf<McpClientRow?>(null) }
    // #EXP-1788: every Revoke is off while one revoke is in flight (the IDE's `revoking`).
    val revoking = pending != null && data?.clients?.any { it.clientId == pending } == true
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
            data == null -> SkeletonRows(modifier = Modifier.padding(vertical = 4.dp), rowHeight = MCP_ROW_HEIGHT)
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
                                userName = memberLabel(client.userName, client.userEmail),
                                kinds = client.kinds,
                                appsAccess = client.appsAccess,
                                lastUsed = client.lastUsedAt?.let { compactRelativeTime(it) },
                            ),
                        )
                        GlassPill(McpSettings.REVOKE, size = PillSize.Sm, onClick = { if (!revoking) target = client }, enabled = !revoking)
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

// ── (3) Access ───────────────────────────────────────────────────────────────

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
                        val name = memberLabel(member.name, member.email)
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

// ── The open Exponential row: its built-in tools ─────────────────────────────

@Composable
private fun BuiltinTools() {
    var query by rememberSaveable { mutableStateOf("") }
    // #EXP-1657: the filter field first, then ONE band per contract tool
    // group (a sub-label over its rows); every band filtered away = the one
    // empty state under the field.
    val bands = remember(query) { McpSettings.builtinToolBands(query) }
    Box(Modifier.padding(start = TOOL_INDENT - 12.dp)) {
        SearchListInput(
            value = query,
            onValueChange = { query = it },
            placeholder = McpSettings.BUILTIN_FILTER_PLACEHOLDER,
            modifier = Modifier.testTag("mcp-builtin-tools-filter"),
        )
    }
    if (bands.isEmpty()) {
        EmptyState(EmptyStateCopy.builtinToolsNoMatch)
    } else {
        bands.forEach { band ->
            ToolSubLabel(band.label)
            band.tools.forEach { ToolRow(it.title, it.blurb, mark = true) }
        }
    }
}
