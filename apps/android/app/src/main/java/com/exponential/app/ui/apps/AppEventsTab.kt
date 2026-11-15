package com.exponential.app.ui.apps

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.Apps
import com.exponential.app.domain.Prompts
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.settings.AgentTriggersSection
import com.exponential.app.ui.settings.middleTruncate
import com.exponential.app.ui.theme.ExpMono
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.delay

// #EXP-1850 (apps.json `eventsTab`, web `components/apps/app-events-tab.tsx`):
// an app's EVENTS tab = ONE scrolling column of three groups, existing
// components only: THE trigger list of the app's agent (the #EXP-1846 piece,
// every kind; editable for the app's creator and team owners), the Webhooks
// group (moved here from Settings) and Recent runs (tRPC `apps.events`: a
// title over a muted detail, the relative time and the status pill; a `woke`
// row opens the Chat tab; none = the muted `noRuns` row).

/** Recent runs are re-read this often while the tab is open (web's 15 s). */
private const val RUNS_REFRESH_MS = 15_000L

@Composable
internal fun AppEventsTab(
    app: AppListEntry,
    viewModel: AppEditorViewModel,
    canManage: Boolean,
    onOpenChat: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val triggers by viewModel.agentTriggers.collectAsStateWithLifecycle()
    val runs by viewModel.eventRuns.collectAsStateWithLifecycle()
    // Re-read on open, every 15 s, and whenever the agent's triggers change.
    LaunchedEffect(app.id, triggers) {
        while (true) {
            viewModel.loadEventRuns()
            delay(RUNS_REFRESH_MS)
        }
    }
    Column(
        verticalArrangement = Arrangement.spacedBy(16.dp),
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(vertical = 8.dp)
            .testTag(Apps.TestIds.EVENTS_TAB),
    ) {
        app.agentId?.let { agentId -> AgentTriggersSection(agentId = agentId, editable = canManage) }
        AppWebhooksGroup(app, viewModel)
        RecentRuns(runs = runs, triggers = triggers, onOpenChat = onOpenChat)
        Spacer(Modifier.height(8.dp))
    }
}

/** Recent runs: the section band over one group of rows. */
@Composable
private fun RecentRuns(
    runs: List<Apps.AppEventRun>?,
    triggers: List<com.exponential.app.domain.AgentTrigger>,
    onOpenChat: () -> Unit,
) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(Modifier.fillMaxWidth().testTag(Apps.TestIds.RECENT_RUNS)) {
        SectionHeader(Apps.EventsTab.RECENT_RUNS, modifier = Modifier.padding(horizontal = 16.dp))
        OptionGroup {
            when {
                runs == null -> Unit
                runs.isEmpty() -> Text(
                    Apps.EventsTab.NO_RUNS,
                    style = MaterialTheme.typography.bodyMedium,
                    color = muted,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                )
                else -> runs.forEachIndexed { index, run ->
                    if (index > 0) GroupDivider()
                    val row = remember(run, triggers) { Apps.eventRunRow(run, triggers) }
                    val woke = row.badge == Apps.AppEventBadge.WOKE
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag(Apps.TestIds.RUN_ROW)
                            .clickable(enabled = woke, onClick = onOpenChat)
                            .padding(horizontal = 16.dp, vertical = 8.dp),
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                row.title,
                                style = MaterialTheme.typography.bodyMedium,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            if (row.detail != null) {
                                Text(
                                    row.detail,
                                    style = MaterialTheme.typography.labelSmall,
                                    color = muted,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }
                        if (run.at.isNotEmpty()) {
                            Text(
                                relativeTime(run.at),
                                style = MaterialTheme.typography.labelSmall,
                                color = muted,
                                maxLines = 1,
                            )
                        }
                        GlassPill(row.badge.label, size = PillSize.Sm)
                    }
                }
            }
        }
    }
}

/** #VAPP-238 Webhooks (#EXP-1850: moved here from the Settings sheet).
 *  Minted secrets live in this tab's memory only (never saved, never
 *  refetched): gone when the tab closes. */
@Composable
private fun AppWebhooksGroup(app: AppListEntry, viewModel: AppEditorViewModel) {
    val hooks by viewModel.hooks.collectAsStateWithLifecycle()
    val canManageHooks by viewModel.canManageHooks.collectAsStateWithLifecycle()
    val hookBusy by viewModel.hookBusy.collectAsStateWithLifecycle()
    var secrets by remember(app.id) { mutableStateOf(mapOf<String, String>()) }
    var rotateTarget by remember(app.id) { mutableStateOf<String?>(null) }
    val clipboard = LocalClipboardManager.current
    val toaster = LocalToaster.current
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    // ── Webhooks (#VAPP-238) ─────────────────────────────────────────
    // Everyone reads the rows and copies a URL; an owner mints (at once)
    // or rotates (asks first) a secret, shown ONCE right under its hook.
    if (hooks.isNotEmpty()) Column(Modifier.fillMaxWidth().testTag(Apps.TestIds.WEBHOOKS)) {
        // #EXP-1850: the same section band as Triggers and Recent runs.
        SectionHeader(Apps.WebhooksCopy.TITLE, modifier = Modifier.padding(horizontal = 16.dp))
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
}
