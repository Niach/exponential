package com.exponential.app.ui.share

import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.exponential.app.domain.ComposerAgentRow
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.ShareTargets
import com.exponential.app.ui.components.AgentBubbleDisc
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.TeamAvatar
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.settings.SettingsRow
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup

/**
 * EXP-1635 (share-targets.json STEP 1): where a share goes. The preview line,
 * New issue + New chat, then the team's agents, all THE settings rows; the
 * team row over them only for a viewer on more than one team.
 */
@Composable
fun ShareToScreen(
    state: ShareToState,
    imageUris: List<Uri>,
    onBack: () -> Unit,
    onSelectTeam: (String) -> Unit,
    onNewIssue: () -> Unit,
    /** The composer's agent row pick: an agent id, or `temporary`. */
    onPick: (String) -> Unit,
) {
    BackHandler(onBack = onBack)
    var teamPickerOpen by remember { mutableStateOf(false) }
    val team = state.teams.firstOrNull { it.id == state.teamId }
    val preview = ShareTargets.previewLabel(imageUris.size)
    Scaffold(
        containerColor = Color.Transparent,
        modifier = Modifier.testTag(ShareTargets.TEST_ID_PAGE),
        topBar = { PageHeader(title = ShareTargets.TITLE, onBack = onBack, buttons = false, modifier = Modifier.statusBarsPadding()) },
    ) { padding ->
        LazyColumn(
            modifier = Modifier.padding(padding).fillMaxSize(),
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (preview != null) {
                item(key = "__preview__") { SharePreviewLine(imageUris, preview) }
            }
            if (state.teams.size > 1 && team != null) {
                item(key = "__team__") {
                    Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                        SettingsRow(
                            icon = ExpIcons.uiChevronRight,
                            title = team.name,
                            leading = { TeamAvatar(iconUrl = team.iconUrl, size = 22.dp) },
                            onClick = { teamPickerOpen = true },
                        )
                    }
                }
            }
            item(key = "__fixed__") {
                Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                    SettingsRow(
                        icon = ExpIcons.navCreateIssue,
                        title = ShareTargets.NEW_ISSUE_LABEL,
                        onClick = onNewIssue,
                        modifier = Modifier.testTag(ShareTargets.rowTestId(ShareTargets.NEW_ISSUE)),
                    )
                    SettingsRow(
                        icon = ExpIcons.uiIconPlaceholder,
                        title = ShareTargets.NEW_CHAT_LABEL,
                        subtitle = ShareTargets.NEW_CHAT_SUBTITLE,
                        onClick = { onPick(ComposerAgentRow.TEMPORARY_PICK) },
                        modifier = Modifier.testTag(ShareTargets.rowTestId(ShareTargets.NEW_CHAT)),
                    )
                }
            }
            if (state.agents.isNotEmpty()) {
                item(key = "__agents__") {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        SectionHeader(ShareTargets.AGENTS_LABEL)
                        Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                            state.agents.forEach { agent ->
                                SettingsRow(
                                    icon = ExpIcons.settingsAgents,
                                    title = agent.name,
                                    leading = { ShareAgentBubble(agent) },
                                    onClick = { onPick(agent.agentId) },
                                    modifier = Modifier.testTag(ShareTargets.rowTestId(agent.agentId)),
                                )
                            }
                        }
                    }
                }
            }
        }
    }
    if (teamPickerOpen) {
        Picker(
            items = state.teams.map { PickerItem(it.id, it.name) },
            mode = PickerMode.Single,
            value = setOfNotNull(state.teamId),
            onChange = { picked -> picked.firstOrNull()?.let(onSelectTeam) },
            title = "Team",
            open = true,
            onOpenChange = { open -> if (!open) teamPickerOpen = false },
        )
    }
}

/** The shared images' thumbnails + their count. */
@Composable
private fun SharePreviewLine(imageUris: List<Uri>, label: String) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp).testTag(ShareTargets.TEST_ID_PREVIEW),
    ) {
        imageUris.take(MAX_THUMBNAILS).forEach { uri ->
            AsyncImage(
                model = uri,
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.size(40.dp).clip(RoundedCornerShape(8.dp)),
            )
        }
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.padding(start = 6.dp),
        )
    }
}

/** THE chats.json agent bubble at the row's lead size. */
@Composable
private fun ShareAgentBubble(agent: SessionTree.AgentRow) {
    AgentBubbleDisc(
        icon = agent.icon,
        color = agent.color,
        assistant = agent.assistant,
        size = 22.dp,
        ring = Color.Transparent,
        ringWidth = 0.dp,
        ringGap = 0.dp,
        glyph = 14.dp,
    )
}

private const val MAX_THUMBNAILS = 4
