package com.exponential.app.ui.actions

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.domain.TriggerWhen
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.actionGlyph
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

// The action page (EXP-1291: an action is a SAVED PROMPT): its editor
// ([ActionPromptTab]) under a header with back, the action's glyph + name and
// Run (the home composer with the action preselected). No triggers (they live
// on agents now) and no runs tab.

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ActionDetailScreen(
    onBack: () -> Unit,
    // EXP-825: Run navigates to the composer with this action preselected.
    onOpenAgent: (AgentComposerSeed) -> Unit,
    viewModel: ActionDetailViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val action = state.action

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = {
                    if (action != null) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            Icon(
                                actionGlyph(action),
                                contentDescription = null,
                                modifier = Modifier.size(18.dp),
                                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                            Text(
                                action.name,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.testTag("action-detail-title"),
                            )
                        }
                    }
                },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                actions = {
                    if (action != null) {
                        CircleIconButton(
                            ExpIcons.actionRun,
                            contentDescription = "Run",
                            onClick = { onOpenAgent(AgentComposerSeed(promptId = action.id)) },
                            modifier = Modifier
                                .padding(end = 8.dp)
                                .testTag("action-detail-run"),
                        )
                    }
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        when {
            // EXP-1275: the first load = skeleton rows, never a spinner.
            action == null && state.loading -> SkeletonRows(
                modifier = Modifier.padding(padding).padding(horizontal = 16.dp, vertical = 8.dp),
            )
            // Deleted meanwhile, or a ref to an action this account can't see.
            action == null -> EmptyState(EmptyStateCopy.actionUnavailable, modifier = Modifier.padding(padding))
            else -> ActionPromptTab(
                actionId = action.id,
                onDeleted = onBack,
                modifier = Modifier
                    .padding(padding)
                    .consumeWindowInsets(padding)
                    .imePadding()
                    .fillMaxSize(),
            )
        }
    }
}

/** The glyph of a trigger kind (the agent Schedule rows). */
internal fun triggerGlyph(whenPart: TriggerWhen): ImageVector = when (whenPart) {
    is TriggerWhen.Schedule -> ExpIcons.triggerSchedule
    is TriggerWhen.Event -> ExpIcons.triggerEvent
}
