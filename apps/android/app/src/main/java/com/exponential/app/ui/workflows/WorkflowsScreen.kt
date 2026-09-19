package com.exponential.app.ui.workflows

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.WorkflowEntity
import com.exponential.app.domain.WorkflowView
import com.exponential.app.domain.shape
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * EXP-981: the team's workflows. A phone has no sidebar, so this is a PUSHED
 * screen off the Agent page's `nav-workflows` button (web and the desktop put
 * the same list in the sidebar right after Automations).
 *
 * Three bands in order — Running, Draft, Done — as the app's standard filled
 * group band over FLAT rows (EXP-818): no row buttons, empty bands hidden,
 * newest first inside a band. A row is the workflow glyph, its name, its shape
 * line, and a warning glyph while its plan holds a cycle.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WorkflowsScreen(
    onBack: () -> Unit,
    onOpenWorkflow: (workflowId: String) -> Unit,
    viewModel: WorkflowsViewModel = hiltViewModel(),
) {
    val bands by viewModel.bands.collectAsStateWithLifecycle()

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(WorkflowView.WORKFLOWS_TITLE) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                    containerColor = Color.Transparent,
                ),
            )
        },
    ) { padding ->
        if (bands.isEmpty()) {
            EmptyState(
                message = WorkflowView.WORKFLOWS_EMPTY_TITLE,
                detail = WorkflowView.WORKFLOWS_EMPTY_BODY,
                icon = ExpIcons.navWorkflows,
                modifier = Modifier.padding(padding),
            )
            return@Scaffold
        }
        LazyColumn(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .testTag("workflows-list"),
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            bands.forEach { band ->
                item(key = "band-${band.band.key}") { SectionHeader(band.band.title) }
                items(band.workflows, key = { it.id }) { workflow ->
                    WorkflowRow(workflow = workflow, onClick = { onOpenWorkflow(workflow.id) })
                }
            }
        }
    }
}

/**
 * One flat list row: the `nav-workflows` glyph, the name, the shape line as
 * its secondary text, and the warning glyph whenever the plan holds a cycle
 * (the note itself lives on the detail, where it can be acted on).
 */
@Composable
private fun WorkflowRow(workflow: WorkflowEntity, onClick: () -> Unit) {
    val shape = remember(workflow.metrics) { workflow.shape }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 10.dp)
            .testTag("workflow-row"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            ExpIcons.navWorkflows,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(10.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                workflow.name,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                WorkflowView.shapeLine(shape),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (shape.cycles.isNotEmpty()) {
            Spacer(Modifier.width(8.dp))
            Icon(
                ExpIcons.uiWarning,
                contentDescription = "This workflow holds a cycle",
                modifier = Modifier.size(16.dp).testTag("workflow-cycle-warning"),
                tint = MaterialTheme.colorScheme.error,
            )
        }
    }
}
