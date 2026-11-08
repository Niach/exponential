package com.exponential.app.ui.chat

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.Chats
import com.exponential.app.domain.PrChecks
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.ChatMenuItem
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuDefaults
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.HeaderEdgeChrome
import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * #EXP-1688 (chats.json `shell.chatHeader`, web `MobileDetailHeader` +
 * `PhoneHeaderTitle`): the ghost button the chat header's back, telephone and
 * `…` all wear: no fill, no hairline, the secondary glyph (the filled circle
 * is the top-level page header's). The tap target stays the platform minimum.
 */
@Composable
fun ChatHeaderButton(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    CircleIconButton(
        icon,
        contentDescription,
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        borderless = true,
        size = Chats.Shell.ChatHeader.BUTTON.dp,
        glyphSize = Chats.Shell.ChatHeader.GLYPH.dp,
    )
}

/**
 * Session 58cb32df (chats.json TRANSCRIPT, phones): THE chat header — back ·
 * the lead disc · the title over its subtitle (S-3: no Stop pill) · the
 * chat's Merge pill ([action]) · NEW-69: the telephone ([call], an agent chat
 * only, left of `…`) · the `…` menu (the chat's own entries,
 * `chatMenuEntries` / `agentMenuEntries`, then any [menuGroups] after a
 * divider). #EXP-1688: one leading row, nothing centred, ghost buttons
 * (`shell.chatHeader`). #EXP-1732: THE gear ([gear]) sits left of `…`, and THE
 * session screen's tabs ([SessionScreenTabs]) ride under it ([tabs]) on the
 * same band.
 */
@Composable
fun ChatHeader(
    title: String,
    subtitle: String?,
    lead: @Composable () -> Unit,
    onBack: (() -> Unit)?,
    modifier: Modifier = Modifier,
    subtitleColor: Color? = null,
    menu: List<ChatMenuItem> = emptyList(),
    /** Further menu groups (the Guide's Close PR), divider-separated. */
    menuGroups: List<List<ChatMenuItem>> = emptyList(),
    tabs: (@Composable () -> Unit)? = null,
    /** NEW-69: the agent chat's telephone (`AgentCallButton`), left of `…`. */
    call: (@Composable () -> Unit)? = null,
    /** S-3: the action slot left of `…` (the chat's Merge pill; it may
     *  draw nothing). */
    action: (@Composable () -> Unit)? = null,
    /** #EXP-1732 (session-screen.json `gear`): THE gear, left of `…`
     *  (agents, the Assistant, apps); null = none (a chat). */
    gear: (() -> Unit)? = null,
    /** #EXP-1732: a kind's OWN `…` (an app's menu sheet: Publish, Export,
     *  Delete) drawn instead of [menu]'s dropdown; null = [menu]. */
    more: (@Composable () -> Unit)? = null,
    /** #EXP-1732: right after the title on its line (an app's version
     *  pill, web/iOS/desktop alike); the title truncates first. */
    titleTrailing: (@Composable () -> Unit)? = null,
) {
    HeaderEdgeChrome {
        Column(modifier = modifier.fillMaxWidth().statusBarsPadding().testTag("chat-header")) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(Chats.Shell.ChatHeader.ROW.dp)
                    .padding(horizontal = Chats.Shell.ChatHeader.PADDING_X.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val gap = Chats.Shell.ChatHeader.GAP.dp
                if (onBack != null) {
                    ChatHeaderButton(ExpIcons.uiBack, "Back", onClick = onBack)
                    Spacer(Modifier.width(gap))
                }
                lead()
                Spacer(Modifier.width(Chats.Shell.ChatHeader.DISC_GAP.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        title,
                        style = MaterialTheme.typography.titleMedium.copy(
                            fontSize = Chats.Shell.ChatHeader.TITLE.sp,
                            lineHeight = 22.sp,
                            fontWeight = FontWeight.SemiBold,
                            letterSpacing = 0.sp,
                        ),
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.testTag("chat-title").weight(1f, fill = false),
                    )
                    if (titleTrailing != null) {
                        Spacer(Modifier.width(6.dp))
                        titleTrailing()
                    }
                    }
                    if (!subtitle.isNullOrEmpty()) {
                        Text(
                            subtitle,
                            style = MaterialTheme.typography.labelMedium.copy(
                                fontSize = Chats.Shell.ChatHeader.SUBTITLE.sp,
                                fontWeight = FontWeight.Normal,
                                letterSpacing = 0.sp,
                            ),
                            color = subtitleColor ?: MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.testTag("chat-subtitle"),
                        )
                    }
                }
                // Draws its own leading gap (nothing while there is no action).
                action?.invoke()
                if (call != null) {
                    Spacer(Modifier.width(gap))
                    call()
                }
                if (gear != null) {
                    Spacer(Modifier.width(gap))
                    SessionGearButton(onClick = gear)
                }
                val groups = listOf(menu).plus(menuGroups).filter { it.isNotEmpty() }
                if (more != null) {
                    Spacer(Modifier.width(gap))
                    more()
                } else if (groups.isNotEmpty()) {
                    Spacer(Modifier.width(gap))
                    ChatHeaderMenu(groups)
                }
            }
            tabs?.invoke()
        }
    }
}

/** The header `…`: a glass dropdown over the entries, groups divided.
 *  #EXP-1611: a submenu entry (Checks) PUSHES its rows as the menu's page
 *  (a back row titled with the entry), the phone arm of THE Menu's submenu. */
@Composable
private fun ChatHeaderMenu(groups: List<List<ChatMenuItem>>) {
    var open by remember { mutableStateOf(false) }
    var page by remember { mutableStateOf<ChatMenuItem?>(null) }
    Box {
        ChatHeaderButton(
            ExpIcons.uiMore,
            "More",
            onClick = {
                page = null
                open = true
            },
            modifier = Modifier.testTag(SessionTree.ChatCopy.TEST_ID_GUIDE_MENU),
        )
        GlassDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            // The pushed page follows the live rows (a running check ticks).
            val pushed = page?.let { p -> groups.flatten().firstOrNull { it.label == p.label && it.submenu != null } }
            ChatMenuPanel(
                groups = groups,
                page = pushed,
                onPush = { page = it },
                onBack = { page = null },
                onPick = { item ->
                    open = false
                    item.onClick()
                },
            )
        }
    }
}

/**
 * The header `…`'s rows ([ChatHeaderMenu]'s dropdown body; the styleguide's
 * `pr-checks` draws the same rows in a static [GlassMenuSurface]): the
 * groups divided, or — [page] pushed — a back row titled with that entry
 * over its submenu rows.
 */
@Composable
fun ChatMenuPanel(
    groups: List<List<ChatMenuItem>>,
    page: ChatMenuItem?,
    onPush: (ChatMenuItem) -> Unit,
    onBack: () -> Unit,
    onPick: (ChatMenuItem) -> Unit,
) {
    if (page != null) {
        GlassMenuItem(
            leadingIcon = { Icon(ExpIcons.uiBack, contentDescription = null, modifier = Modifier.size(GlassMenuDefaults.IconSize)) },
            text = { Text(page.label) },
            onClick = onBack,
        )
        HorizontalDivider(color = GlassMenuDefaults.DividerColor)
        page.submenu.orEmpty().forEach { item -> ChatMenuPanelRow(item, onClick = { onPick(item) }) }
        return
    }
    groups.forEachIndexed { index, group ->
        if (index > 0) HorizontalDivider(color = GlassMenuDefaults.DividerColor)
        group.forEach { item ->
            ChatMenuPanelRow(item, onClick = { if (item.submenu != null) onPush(item) else onPick(item) })
        }
    }
}

@Composable
private fun ChatMenuPanelRow(item: ChatMenuItem, onClick: () -> Unit) {
    GlassMenuItem(
        leadingIcon = {
            if (item.spinner) {
                InlineSpinner(size = GlassMenuDefaults.IconSize, color = item.iconTint ?: GlassMenuDefaults.IconColor)
            } else if (item.iconTint != null) {
                Icon(item.icon, contentDescription = null, tint = item.iconTint, modifier = Modifier.size(GlassMenuDefaults.IconSize))
            } else {
                Icon(item.icon, contentDescription = null, modifier = Modifier.size(GlassMenuDefaults.IconSize))
            }
        },
        text = { Text(item.label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        trailing = if (item.value != null || item.submenu != null) {
            { ChatMenuTrailing(item.value, chevron = item.submenu != null) }
        } else {
            null
        },
        destructive = item.destructive,
        enabled = item.enabled,
        onClick = onClick,
        modifier = item.testTag?.let { Modifier.testTag(it) } ?: Modifier,
    )
}

/** A row's muted value, then (a submenu entry) the chevron. */
@Composable
private fun ChatMenuTrailing(value: String?, chevron: Boolean) {
    val muted = Color.White.copy(alpha = TextEmphasis.Tertiary)
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (value != null) {
            Text(value, style = MaterialTheme.typography.bodyMedium, color = muted, maxLines = 1)
        }
        if (chevron) {
            if (value != null) Spacer(Modifier.width(6.dp))
            Icon(ExpIcons.uiChevronRight, contentDescription = null, modifier = Modifier.size(GlassMenuDefaults.IconSize), tint = muted)
        }
    }
}

/** A check glyph's concept icon (pr-checks.json GLYPHS). */
fun prCheckIcon(glyph: PrChecks.Glyph): ImageVector = when (glyph) {
    PrChecks.Glyph.Queued -> ExpIcons.ciQueued
    PrChecks.Glyph.Running -> ExpIcons.ciRunning
    PrChecks.Glyph.Passed -> ExpIcons.ciPassed
    PrChecks.Glyph.Failed -> ExpIcons.ciFailed
    PrChecks.Glyph.Skipped -> ExpIcons.ciSkipped
}

/** A check tone's colour: the semantic green / red tokens, muted otherwise. */
fun prCheckTint(tone: PrChecks.Tone): Color = when (tone) {
    PrChecks.Tone.Green -> DesignTokens.Semantic.Green
    PrChecks.Tone.Red -> DesignTokens.Semantic.Red
    PrChecks.Tone.Muted -> GlassMenuDefaults.IconColor
}

/**
 * #EXP-1611 (pr-checks.json CHECKS LIST): the header `…`'s leading Checks
 * submenu entry — the summary glyph tinted by its tone, `Checks`, the
 * summary value — over [PrChecks.checksList]'s rows (glyph tinted, a running
 * check = THE Spinner, the name, the duration; a pick opens the logs via
 * [onOpen], a row without a url is disabled). Null = no rows (no entry).
 */
fun prChecksMenuItem(checks: List<PrChecks.Check>, nowMs: Long, onOpen: (String) -> Unit): ChatMenuItem? {
    val summary = PrChecks.checksSummary(checks) ?: return null
    val rows = PrChecks.checksList(checks, nowMs).map { row ->
        ChatMenuItem(
            label = row.name,
            icon = prCheckIcon(row.glyph),
            iconTint = prCheckTint(row.tone),
            spinner = row.glyph == PrChecks.Glyph.Running,
            value = row.duration,
            enabled = row.url != null,
            onClick = { row.url?.let(onOpen) },
            testTag = PrChecks.TEST_ID_CHECK_ROW,
        )
    }
    return ChatMenuItem(
        label = PrChecks.Copy.CHECKS,
        icon = prCheckIcon(summary.glyph),
        iconTint = prCheckTint(summary.tone),
        spinner = summary.glyph == PrChecks.Glyph.Running,
        value = summary.value,
        onClick = {},
        testTag = PrChecks.TEST_ID_CHECKS_MENU,
        submenu = rows,
    )
}
