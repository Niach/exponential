package com.exponential.app.ui.thread

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.ChatMenuItem
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuDefaults
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.HeaderEdgeChrome
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * Thread 58cb32df (chats.json TRANSCRIPT, phones): THE chat header — back ·
 * the lead disc · the title over its subtitle (S-3: no Stop pill) · NEW-69: the
 * telephone ([call], an agent chat only, left of `…`) · the `…` menu (the
 * chat's own entries, `threadMenuEntries` / `agentMenuEntries`, then any
 * [menuGroups] after a divider). The thread's tab strip rides under it
 * ([tabs]) on the same band.
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
) {
    HeaderEdgeChrome {
        Column(modifier = modifier.fillMaxWidth().statusBarsPadding().testTag("thread-header")) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, top = 6.dp, bottom = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (onBack != null) {
                    CircleIconButton(ExpIcons.uiBack, "Back", onClick = onBack, size = 44.dp, glyphSize = 18.dp)
                    Spacer(Modifier.width(10.dp))
                }
                lead()
                Spacer(Modifier.width(10.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        title,
                        style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.SemiBold),
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.testTag("thread-title"),
                    )
                    if (!subtitle.isNullOrEmpty()) {
                        Text(
                            subtitle,
                            style = MaterialTheme.typography.labelMedium,
                            color = subtitleColor ?: MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.testTag("thread-subtitle"),
                        )
                    }
                }
                if (call != null) {
                    Spacer(Modifier.width(8.dp))
                    call()
                }
                val groups = listOf(menu).plus(menuGroups).filter { it.isNotEmpty() }
                if (groups.isNotEmpty()) {
                    Spacer(Modifier.width(8.dp))
                    ChatHeaderMenu(groups)
                }
            }
            tabs?.invoke()
        }
    }
}

/** The header `…`: a glass dropdown over the entries, groups divided. */
@Composable
private fun ChatHeaderMenu(groups: List<List<ChatMenuItem>>) {
    var open by remember { mutableStateOf(false) }
    Box {
        CircleIconButton(
            ExpIcons.uiMore,
            "More",
            onClick = { open = true },
            size = 44.dp,
            glyphSize = 18.dp,
            modifier = Modifier.testTag(SessionTree.ThreadCopy.TEST_ID_GUIDE_MENU),
        )
        GlassDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            groups.forEachIndexed { index, group ->
                if (index > 0) HorizontalDivider(color = GlassMenuDefaults.DividerColor)
                group.forEach { item ->
                    GlassMenuItem(
                        leadingIcon = { Icon(item.icon, contentDescription = null, modifier = Modifier.size(GlassMenuDefaults.IconSize)) },
                        text = { Text(item.label) },
                        destructive = item.destructive,
                        onClick = {
                            open = false
                            item.onClick()
                        },
                        modifier = item.testTag?.let { Modifier.testTag(it) } ?: Modifier,
                    )
                }
            }
        }
    }
}
