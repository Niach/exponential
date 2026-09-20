package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MdStyle

/**
 * EXP-920: the entity badge — the SAME chip as [IssueChip] (one [ChipShell])
 * for every other kind an Exponential tool's answer names: a board, an
 * action, a run, a label… A glyph, then the label. An ISSUE ref with an
 * identifier reads exactly like [IssueChip]: the identifier in muted mono
 * ([monoLabel]) and the title as the medium [detail] beside it; anything else
 * is glyph + label in the body colour.
 *
 * [icon] is a slot, not a glyph: an issue paints its resolved status glyph
 * once its row is synced, a board its coloured board icon, a label its dot.
 */
@Composable
fun EntityChip(
    icon: @Composable () -> Unit,
    label: String,
    detail: String?,
    onClick: (() -> Unit)?,
    modifier: Modifier = Modifier,
    /** Draw [label] in the identifier style (mono, muted) rather than the
     *  body style — an issue named by its identifier. */
    monoLabel: Boolean = false,
) {
    ChipShell(modifier = modifier, onClick = onClick) {
        icon()
        Text(
            label,
            style = IssueChipDefaults.textStyle().let {
                if (monoLabel) it.copy(fontFamily = FontFamily.Monospace) else it
            },
            color = if (monoLabel) MdStyle.ChipToken else MdStyle.Text,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (detail != null) {
            Text(
                detail,
                style = IssueChipDefaults.textStyle(),
                color = MdStyle.Text,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** A chip's glyph at the painter's glyph size, tinted like the token text. */
@Composable
fun EntityChipGlyph(icon: ImageVector, tint: androidx.compose.ui.graphics.Color = MdStyle.ChipToken) {
    Box(Modifier.size(MdStyle.chipIconSize), contentAlignment = Alignment.Center) {
        Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(MdStyle.chipIconSize))
    }
}

/**
 * EXP-920: the icon CONCEPT an entity-preview kind names
 * (`EntityPreview.ICON`, `packages/icons` `semantic`) as this build's glyph.
 * An explicit `when` — the registry generates concept ACCESSORS, not a
 * by-name lookup — locked by `EntityChipIconTest` so a concept the contract
 * adds can never fall through to null.
 */
fun entityConceptIcon(concept: String): ImageVector? = when (concept) {
    "ui-issue" -> ExpIcons.uiIssue
    "nav-boards" -> ExpIcons.navBoards
    "nav-actions" -> ExpIcons.navActions
    "nav-automations" -> ExpIcons.navAutomations
    "notification-issue-comment" -> ExpIcons.notificationIssueComment
    "coding-running" -> ExpIcons.codingRunning
    "settings-labels" -> ExpIcons.settingsLabels
    "settings-statuses" -> ExpIcons.settingsStatuses
    "nav-workflows" -> ExpIcons.navWorkflows
    "ui-device" -> ExpIcons.uiDevice
    "ui-avatar-placeholder" -> ExpIcons.uiAvatarPlaceholder
    "ui-repository" -> ExpIcons.uiRepository
    "ui-team" -> ExpIcons.uiTeam
    "ui-invite" -> ExpIcons.uiInvite
    "nav-notifications" -> ExpIcons.navNotifications
    "nav-support" -> ExpIcons.navSupport
    "ui-attach" -> ExpIcons.uiAttach
    "ui-checklist" -> ExpIcons.uiChecklist
    else -> null
}
