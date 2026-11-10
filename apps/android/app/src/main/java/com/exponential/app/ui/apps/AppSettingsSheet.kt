package com.exponential.app.ui.apps

import android.net.Uri
import com.exponential.app.ui.theme.ExpMono
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import at.exponential.ui.json.JsonValue
import com.exponential.app.domain.AppActions
import com.exponential.app.domain.AppRecords
import com.exponential.app.domain.AppShare
import com.exponential.app.domain.Apps
import com.exponential.app.domain.Prompts
import com.exponential.app.domain.shareRowSubtitle
import com.exponential.app.domain.shareSwitchOn
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.ColorPicker
import com.exponential.app.ui.components.DestructiveButton
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSwitch
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.components.picker.InlinePickerRow
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.settings.middleTruncate
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis

// VAPP-177 (fixtures/apps.json THE SETTINGS TAB, web
// `components/apps/app-settings-tab.tsx`): phones open the editor's Settings
// tab as the `…` menu's sheet, grants included: the identity row (icon +
// colour + name, the name a deferred write on focus loss / Done), the
// description (saved on focus loss), the records line (app-records.json
// `recordsCountLabel`), the Tools group (app-actions.json: one row per
// function the head package declares, a Switch for a team OWNER only, a
// destructive action `Always asks`), then Export (a version exists) and
// Delete app (the creator or an owner). Everyone else reads it.

private const val DEFAULT_COLOR = "#6366f1"

@Composable
internal fun AppSettingsBody(
    app: AppListEntry,
    viewModel: AppEditorViewModel,
    records: Int,
    canManage: Boolean,
    canGrant: Boolean,
    onExport: () -> Unit,
    onDelete: () -> Unit,
) {
    val granted by viewModel.grantedTools.collectAsStateWithLifecycle()
    val grantBusy by viewModel.grantBusy.collectAsStateWithLifecycle()
    val automations by viewModel.automations.collectAsStateWithLifecycle()
    val automationSponsors by viewModel.automationSponsors.collectAsStateWithLifecycle()
    val accounts by viewModel.accounts.collectAsStateWithLifecycle()
    val upstreamServers by viewModel.upstreamServers.collectAsStateWithLifecycle()
    val upstreamBusy by viewModel.upstreamBusy.collectAsStateWithLifecycle()
    val mcpTools by viewModel.mcpTools.collectAsStateWithLifecycle()
    val mcpToolBusy by viewModel.mcpToolBusy.collectAsStateWithLifecycle()
    val context = LocalContext.current
    // #VAPP-243: back from the browser's Connect = re-read my accounts.
    var resumedOnce by remember(app.id) { mutableStateOf(false) }
    LifecycleResumeEffect(app.id) {
        if (resumedOnce) viewModel.refreshTools()
        resumedOnce = true
        onPauseOrDispose {}
    }
    val hooks by viewModel.hooks.collectAsStateWithLifecycle()
    val canManageHooks by viewModel.canManageHooks.collectAsStateWithLifecycle()
    val hookBusy by viewModel.hookBusy.collectAsStateWithLifecycle()
    val shares by viewModel.shares.collectAsStateWithLifecycle()
    val shareBusy by viewModel.shareBusy.collectAsStateWithLifecycle()
    // #VAPP-238: minted secrets live in this screen's memory only (never
    // saved, never refetched): gone when the tab or sheet closes.
    var secrets by remember(app.id) { mutableStateOf(mapOf<String, String>()) }
    var rotateTarget by remember(app.id) { mutableStateOf<String?>(null) }
    val clipboard = LocalClipboardManager.current
    val toaster = LocalToaster.current
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val focus = LocalFocusManager.current
    // Seeded per APP, never on a resync of the same one.
    var name by rememberSaveable(app.id) { mutableStateOf(app.name) }
    var icon by rememberSaveable(app.id) { mutableStateOf(app.icon) }
    var color by rememberSaveable(app.id) { mutableStateOf(app.color ?: DEFAULT_COLOR) }
    var description by rememberSaveable(app.id) { mutableStateOf(app.description) }
    val saveName = {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) name = app.name else if (trimmed != app.name) viewModel.update(name = trimmed)
    }
    val saveDescription = { if (description != app.description) viewModel.update(description = description) }
    // VAPP-120: the typed actions the RUNNING package declares (eApps V1: the
    // published one, else the head's); FEED-159: a stored grant that is no
    // longer grantable reads as none.
    val grantPackage = app.packageJson ?: app.draftPackageJson
    val declared = remember(grantPackage) {
        val functions = grantPackage
            ?.let { runCatching { JsonValue.parse(it) }.getOrNull() }
            ?.get("functions")?.array?.mapNotNull { it.string }
            .orEmpty()
        AppActions.declaredActions(functions)
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .testTag(Apps.TestIds.SETTINGS),
    ) {
        OptionGroup {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxWidth().padding(start = 12.dp, top = 6.dp, bottom = 6.dp),
            ) {
                if (canManage) {
                    IconPicker(
                        selected = icon,
                        onSelect = { next ->
                            icon = next
                            viewModel.update(icon = next)
                        },
                        accentColor = parseColor(color),
                    )
                    ColorPicker(
                        selected = color,
                        onSelect = { next ->
                            color = next
                            viewModel.update(color = next)
                        },
                    )
                } else {
                    Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { AppGlyph(app.icon, app.color, size = 20.dp) }
                }
                GlassTextField(
                    value = name,
                    onValueChange = { name = it.take(Apps.NAME_MAX) },
                    placeholder = Apps.Copy.NAME_PLACEHOLDER,
                    enabled = canManage,
                    singleLine = true,
                    bordered = false,
                    containerColor = Color.Transparent,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
                    modifier = Modifier
                        .weight(1f)
                        .testTag("app-settings-name")
                        .commitOnBlur(saveName),
                )
            }
        }
        Spacer(Modifier.height(12.dp))
        OptionGroup {
            GlassTextField(
                value = description,
                onValueChange = { description = it.take(Apps.DESCRIPTION_MAX) },
                placeholder = Apps.Copy.DESCRIPTION_PLACEHOLDER,
                minLines = 3,
                enabled = canManage,
                bordered = false,
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("app-settings-description")
                    .commitOnBlur(saveDescription),
            )
        }
        // VAPP-122: the count; no plan quota on phones (null = the plain count).
        Text(
            AppRecords.recordsCountLabel(records, null),
            style = MaterialTheme.typography.bodyMedium,
            color = muted,
            modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 8.dp).testTag("app-settings-records"),
        )
        Spacer(Modifier.height(16.dp))

        // ── Record access (#VAPP-243) ────────────────────────────────────
        // Read-only: each declared kind (mono) over its effective policy.
        val policies = remember(grantPackage) {
            Apps.policyRows(grantPackage?.let { runCatching { JsonValue.parse(it) }.getOrNull() })
        }
        if (policies.isNotEmpty()) Column(Modifier.fillMaxWidth().testTag(Apps.TestIds.POLICIES)) {
            Text(
                Apps.PoliciesCopy.TITLE,
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.padding(horizontal = 20.dp),
            )
            Spacer(Modifier.height(8.dp))
            OptionGroup {
                policies.forEachIndexed { index, policy ->
                    if (index > 0) GroupDivider()
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .testTag("${Apps.TestIds.POLICY_ROW}-${policy.kind}")
                            .padding(horizontal = 16.dp, vertical = 8.dp),
                    ) {
                        Text(
                            policy.kind,
                            style = MaterialTheme.typography.bodyMedium,
                            fontFamily = ExpMono,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(policy.caption, style = MaterialTheme.typography.labelSmall, color = muted)
                    }
                }
            }
            Spacer(Modifier.height(16.dp))
        }

        // ── Tools (VAPP-120) ─────────────────────────────────────────────
        Column(Modifier.padding(horizontal = 20.dp).testTag("app-settings-tools")) {
            Text(AppActions.Copy.TITLE, style = MaterialTheme.typography.titleSmall)
            Text(AppActions.Copy.HINT, style = MaterialTheme.typography.bodyMedium, color = muted)
        }
        Spacer(Modifier.height(8.dp))
        OptionGroup {
            if (declared.isEmpty()) {
                Text(
                    AppActions.Copy.NONE,
                    style = MaterialTheme.typography.bodyMedium,
                    color = muted,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                )
            }
            declared.forEachIndexed { index, tool ->
                if (index > 0) GroupDivider()
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("app-tool-$tool")
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(
                            AppActions.actionTitle(tool),
                            style = MaterialTheme.typography.bodyMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            tool,
                            style = MaterialTheme.typography.labelSmall,
                            fontFamily = ExpMono,
                            color = muted,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    if (AppActions.actionGrantable(tool)) {
                        val editable = canGrant && grantBusy != tool
                        GlassSwitch(
                            checked = tool in granted,
                            onCheckedChange = if (editable) ({ on -> viewModel.setGrant(tool, on) }) else null,
                            enabled = editable,
                        )
                    } else {
                        // The ruling: a destructive or owner-only action is never grantable.
                        Text(AppActions.Copy.ALWAYS, style = MaterialTheme.typography.bodyMedium, color = muted)
                    }
                }
            }
        }
        Spacer(Modifier.height(16.dp))

        // ── Public links (VAPP-241) ──────────────────────────────────────
        // Everyone reads the rows; an owner switches a link and, while it is
        // on, copies it or mints a new one from the `…` menu.
        if (shares.isNotEmpty()) Column(Modifier.fillMaxWidth().testTag(AppShare.TestIds.SHARE_LINKS)) {
            Text(
                AppShare.Copy.TITLE,
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.padding(horizontal = 20.dp),
            )
            Spacer(Modifier.height(8.dp))
            OptionGroup {
                shares.forEachIndexed { index, share ->
                    if (index > 0) GroupDivider()
                    val on = share.state == AppShare.State.ON
                    val url = share.url
                    val editable = canManageHooks && shareBusy == null
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("${AppShare.TestIds.SHARE_ROW}-${share.name}")
                            .padding(horizontal = 16.dp, vertical = 8.dp),
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                share.name,
                                style = MaterialTheme.typography.bodyMedium,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Text(
                                shareRowSubtitle(share.mode, share.state, share.expiresAt),
                                style = MaterialTheme.typography.labelSmall,
                                color = muted,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        if (canManageHooks && on && url != null) {
                            var menu by remember(share.name) { mutableStateOf(false) }
                            Box {
                                CircleIconButton(
                                    ExpIcons.uiMore,
                                    contentDescription = "Menu for ${share.name}",
                                    onClick = { menu = true },
                                    borderless = true,
                                    modifier = Modifier.testTag("${AppShare.TestIds.SHARE_ROW}-${share.name}-menu"),
                                )
                                GlassDropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                                    GlassMenuItem(
                                        leadingIcon = { Icon(ExpIcons.uiCopy, contentDescription = null) },
                                        text = { Text(AppShare.Copy.COPY_LINK) },
                                        onClick = {
                                            menu = false
                                            clipboard.setText(AnnotatedString(url))
                                            toaster.success(Apps.WebhooksCopy.COPIED)
                                        },
                                    )
                                    GlassMenuItem(
                                        leadingIcon = { Icon(ExpIcons.uiRefresh, contentDescription = null) },
                                        text = { Text(AppShare.Copy.NEW_LINK) },
                                        enabled = editable,
                                        onClick = {
                                            menu = false
                                            viewModel.setShareLink(share.name, AppShare.Action.ROTATE)
                                        },
                                    )
                                    AppShare.EXPIRY_CHOICES.forEach { choice ->
                                        GlassMenuItem(
                                            leadingIcon = { Icon(ExpIcons.uiClock, contentDescription = null) },
                                            text = { Text(choice.label) },
                                            enabled = editable,
                                            onClick = {
                                                menu = false
                                                viewModel.setShareLink(share.name, AppShare.Action.EXPIRE, choice.days)
                                            },
                                        )
                                    }
                                }
                            }
                        }
                        if (canManageHooks) {
                            GlassSwitch(
                                checked = shareSwitchOn(share.state),
                                onCheckedChange = if (editable) ({ next ->
                                    viewModel.setShareLink(share.name, if (next) AppShare.Action.ENABLE else AppShare.Action.REVOKE)
                                }) else null,
                                enabled = editable,
                            )
                        }
                    }
                }
            }
            Spacer(Modifier.height(16.dp))
        }

        // ── Accounts (#VAPP-243) ─────────────────────────────────────────
        // The upstreams the app declares, each with my OWN connection:
        // the muted state, or Connect / Reconnect (the web page in the browser).
        // #VAPP-254 (`upstreamPickerCases`): the real server on a second muted
        // line when it differs; an owner's server picker before the trailing state.
        if (accounts.isNotEmpty()) Column(Modifier.fillMaxWidth().testTag(Apps.TestIds.ACCOUNTS)) {
            Text(
                Apps.AccountsCopy.TITLE,
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.padding(horizontal = 20.dp),
            )
            Spacer(Modifier.height(8.dp))
            OptionGroup {
                accounts.forEachIndexed { index, account ->
                    if (index > 0) GroupDivider()
                    val row = Apps.accountRow(account.toRowInput())
                    val caption = Apps.accountCaption(account.name, account.serverName)
                    // N-14: inside a sheet the picker's rows open under the whole row.
                    InlinePickerRow(
                        modifier = Modifier.testTag("${Apps.TestIds.ACCOUNT_ROW}-${account.name}"),
                        rowModifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        scroll = false,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                account.name,
                                style = MaterialTheme.typography.bodyMedium,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            if (caption != null) {
                                Text(
                                    caption,
                                    style = MaterialTheme.typography.bodySmall,
                                    color = muted,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }
                        UpstreamPickerPill(
                            name = account.name,
                            serverId = account.serverId,
                            servers = upstreamServers,
                            testTag = "${Apps.TestIds.ACCOUNT_ROW}-${account.name}-picker",
                            enabled = upstreamBusy == null,
                            onPick = { viewModel.setUpstreamMapping(account.name, it) },
                        )
                        val serverId = account.serverId
                        if (row.action != null && row.actionLabel != null && serverId != null) {
                            GlassPill(
                                row.actionLabel,
                                size = PillSize.Sm,
                                icon = if (row.action == Apps.AccountAction.RECONNECT) ExpIcons.uiWarning else ExpIcons.uiSignIn,
                                onClick = {
                                    viewModel.connectUrl(serverId)?.let { CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(it)) }
                                },
                                modifier = Modifier.testTag("${Apps.TestIds.ACCOUNT_ROW}-${account.name}-${row.action.wire}"),
                            )
                        } else if (row.label != null && (row.status != "notSetUp" || upstreamServers.isEmpty())) {
                            Text(row.label, style = MaterialTheme.typography.bodyMedium, color = muted)
                        }
                    }
                }
            }
            Spacer(Modifier.height(16.dp))
        }

        // ── MCP tools (#VAPP-243) ────────────────────────────────────────
        // The exposed tools: title over the name, Read / Write, the switch
        // (an owner; everyone else reads it).
        if (mcpTools.isNotEmpty()) Column(Modifier.fillMaxWidth().testTag(Apps.TestIds.MCP_TOOLS)) {
            Text(
                Apps.McpToolsCopy.TITLE,
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.padding(horizontal = 20.dp),
            )
            Spacer(Modifier.height(8.dp))
            OptionGroup {
                mcpTools.forEachIndexed { index, tool ->
                    if (index > 0) GroupDivider()
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("${Apps.TestIds.MCP_TOOL_ROW}-${tool.name}")
                            .padding(horizontal = 16.dp, vertical = 8.dp),
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                tool.title,
                                style = MaterialTheme.typography.bodyMedium,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Text(
                                tool.name,
                                style = MaterialTheme.typography.labelSmall,
                                fontFamily = ExpMono,
                                color = muted,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        GlassPill(if (tool.access == "read") Apps.McpToolsCopy.READ else Apps.McpToolsCopy.WRITE, size = PillSize.Sm)
                        val editable = canManageHooks && mcpToolBusy == null
                        GlassSwitch(
                            checked = tool.enabled,
                            onCheckedChange = if (editable) ({ on -> viewModel.setToolEnabled(tool.name, on) }) else null,
                            enabled = editable,
                        )
                    }
                }
            }
            Spacer(Modifier.height(16.dp))
        }

        // ── Automations (#VAPP-220) ──────────────────────────────────────
        // Everyone reads it, nothing to switch: name, trigger, status pill.
        if (automations.isNotEmpty()) Column(Modifier.fillMaxWidth().testTag(Apps.TestIds.AUTOMATIONS)) {
            Text(
                Apps.AutomationsCopy.TITLE,
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.padding(horizontal = 20.dp),
            )
            Spacer(Modifier.height(8.dp))
            OptionGroup {
                automations.forEachIndexed { index, automation ->
                    if (index > 0) GroupDivider()
                    val row = Apps.automationRow(automation)
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag(Apps.TestIds.AUTOMATION_ROW)
                            .padding(horizontal = 16.dp, vertical = 8.dp),
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                automation.name,
                                style = MaterialTheme.typography.bodyMedium,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Text(
                                automationSponsors[automation.name]?.let { "${row.trigger} · $it" } ?: row.trigger,
                                style = MaterialTheme.typography.labelSmall,
                                color = muted,
                                // The pause reason sits after the trigger: one line cut it off.
                                maxLines = 2,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        GlassPill(row.label, size = PillSize.Sm)
                    }
                }
            }
            Spacer(Modifier.height(16.dp))
        }

        // ── Webhooks (#VAPP-238) ─────────────────────────────────────────
        // Everyone reads the rows and copies a URL; an owner mints (at once)
        // or rotates (asks first) a secret, shown ONCE right under its hook.
        if (hooks.isNotEmpty()) Column(Modifier.fillMaxWidth().testTag(Apps.TestIds.WEBHOOKS)) {
            Text(
                Apps.WebhooksCopy.TITLE,
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.padding(horizontal = 20.dp),
            )
            Spacer(Modifier.height(8.dp))
            OptionGroup {
                hooks.forEachIndexed { index, hook ->
                    if (index > 0) GroupDivider()
                    val secret = secrets[hook.name]
                    val row = Apps.webhookRow(
                        Apps.WebhookRowInput(hook.name, hook.hasSecret || secret != null),
                        canManageHooks,
                    )
                    val act = {
                        viewModel.rotateHook(hook.name) { minted -> secrets = secrets + (hook.name to minted) }
                    }
                    Column(
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("${Apps.TestIds.WEBHOOK_ROW}-${hook.name}")
                            .padding(horizontal = 16.dp, vertical = 8.dp),
                    ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                hook.name,
                                style = MaterialTheme.typography.bodyMedium,
                                fontFamily = ExpMono,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Text(
                                remember(hook.url) { middleTruncate(hook.url) },
                                style = MaterialTheme.typography.labelSmall,
                                fontFamily = ExpMono,
                                color = muted,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        GlassPill(row.label, size = PillSize.Sm)
                    }
                    // The pills on a line of their own, leading-aligned.
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        GlassPill(
                            Apps.WebhooksCopy.COPY_URL,
                            size = PillSize.Sm,
                            onClick = {
                                clipboard.setText(AnnotatedString(hook.url))
                                toaster.success(Apps.WebhooksCopy.COPIED)
                            },
                        )
                        if (row.action != null && row.actionLabel != null) {
                            GlassPill(
                                row.actionLabel,
                                size = PillSize.Sm,
                                enabled = hookBusy == null,
                                loading = hookBusy == hook.name,
                                onClick = {
                                    if (row.action == Apps.WebhookAction.ROTATE) rotateTarget = hook.name else act()
                                },
                                modifier = Modifier.testTag("${Apps.TestIds.WEBHOOK_ROW}-${hook.name}-${row.action.wire}"),
                            )
                        }
                    }
                    }
                    if (secret != null) {
                        GroupDivider()
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(12.dp),
                            modifier = Modifier
                                .fillMaxWidth()
                                .testTag(Apps.TestIds.WEBHOOK_SECRET)
                                .padding(horizontal = 16.dp, vertical = 8.dp),
                        ) {
                            Column(Modifier.weight(1f)) {
                                Text(
                                    Apps.WebhooksCopy.SECRET,
                                    style = MaterialTheme.typography.labelSmall,
                                    color = muted,
                                    maxLines = 1,
                                )
                                Text(
                                    remember(secret) { middleTruncate(secret) },
                                    style = MaterialTheme.typography.bodyMedium,
                                    fontFamily = ExpMono,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                            GlassPill(
                                Apps.WebhooksCopy.COPY_SECRET,
                                size = PillSize.Sm,
                                onClick = {
                                    clipboard.setText(AnnotatedString(secret))
                                    toaster.success(Apps.WebhooksCopy.COPIED)
                                },
                            )
                        }
                    }
                }
            }
            Spacer(Modifier.height(16.dp))
        }
        rotateTarget?.let { hookName ->
            PromptAlert(
                prompt = Prompts.RotateWebhookSecret.prompt(hookName),
                onDismiss = { rotateTarget = null },
                handlers = mapOf(
                    "rotate" to {
                        rotateTarget = null
                        viewModel.rotateHook(hookName) { minted -> secrets = secrets + (hookName to minted) }
                    },
                ),
            )
        }

        // ── Export + Delete app ──────────────────────────────────────────
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
        ) {
            if (app.headVersion > 0) {
                GlassPill(
                    Apps.Copy.MENU_EXPORT,
                    icon = ExpIcons.uiShare,
                    onClick = onExport,
                    modifier = Modifier.testTag("app-export"),
                )
            }
            // EXP-1554: THE destructive button, beside Export, left-aligned (web).
            if (canManage) {
                DestructiveButton(
                    Apps.Copy.MENU_DELETE,
                    onClick = onDelete,
                    modifier = Modifier.testTag("app-delete"),
                )
            }
        }
        Spacer(Modifier.height(8.dp))
    }
}

/** [onBlur] fires when focus LEAVES the field (never on the first,
 *  unfocused report): the save-on-blur. */
@Composable
private fun Modifier.commitOnBlur(onBlur: () -> Unit): Modifier {
    var hadFocus by remember { mutableStateOf(false) }
    return onFocusChanged { state ->
        if (hadFocus && !state.hasFocus) onBlur()
        hadFocus = state.hasFocus
    }
}
