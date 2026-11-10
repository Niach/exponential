package com.exponential.app.ui.settings

import androidx.compose.foundation.clickable
import com.exponential.app.ui.theme.ExpMono
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.SkillOption
import com.exponential.app.domain.Skills
import com.exponential.app.domain.skillPill
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSwitch
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.TopBarTitle
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup

// #EXP-1661: Settings › Skills on a phone, READ ONLY (skills.json `list` +
// `page`, `permissions.phone`): the list = the Skills band over one glass
// group of rows built like the MCP server rows (the `ui-docs` glyph, the
// name + the Default pill (Built-in on a built-in, #VAPP-213), the description as a one-line caption); a row
// opens the skill's page = the prompt page's layout read-only (name, the
// description, the SKILL.md body in mono, Pre-select in new sessions). No New skill, no
// row menu, no Save / Delete: web and desktop own the writes.

/** A glass group edge to edge under its band (the sheets' OptionGroup insets). */
@Composable
private fun SkillGroup(content: @Composable () -> Unit) {
    Column(modifier = Modifier.fillMaxWidth().glassGroup()) { content() }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SkillsPageScaffold(title: String, onBack: () -> Unit, testTag: String, content: @Composable () -> Unit) {
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
        ) { content() }
    }
}

/** Settings › Skills (`team-settings/skills`). */
@Composable
fun SkillsSettingsScreen(
    onBack: () -> Unit,
    onOpenSkill: (skillId: String) -> Unit,
    viewModel: SkillsViewModel = hiltViewModel(key = "skills-list"),
) {
    val skills by viewModel.list.collectAsStateWithLifecycle()
    SkillsPageScaffold(Skills.TITLE, onBack, Skills.PAGE_TEST_ID) {
        SkillsSettingsList(skills = skills, onOpenSkill = onOpenSkill)
    }
}

/** The stateless band: null [skills] = still loading (skeleton rows). */
@Composable
internal fun SkillsSettingsList(skills: List<SkillOption>?, onOpenSkill: (skillId: String) -> Unit) {
    Column(Modifier.testTag(Skills.LIST_TEST_ID)) {
        SectionHeader(Skills.TITLE)
        when {
            skills == null -> SkeletonRows(modifier = Modifier.padding(vertical = 4.dp))
            skills.isEmpty() -> EmptyState(EmptyStateCopy.skills)
            else -> Column(Modifier.fillMaxWidth().glassGroup()) {
                skills.forEachIndexed { i, skill ->
                    if (i > 0) GroupDivider()
                    SkillRow(skill, onClick = { onOpenSkill(skill.id) })
                }
            }
        }
    }
}

@Composable
private fun SkillRow(skill: SkillOption, onClick: () -> Unit) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag(Skills.ROW_TEST_ID)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Icon(ExpIcons.uiDocs, contentDescription = null, modifier = Modifier.size(16.dp), tint = muted)
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(
                    skill.name,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Medium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                skillPill(skill.builtin, skill.enabledByDefault)?.let { GlassPill(it, size = PillSize.Sm) }
            }
            if (skill.description.isNotEmpty()) {
                Text(
                    skill.description,
                    style = MaterialTheme.typography.bodySmall,
                    color = muted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/** ONE skill's page (`settings/skill/{skillId}`), read only. */
@Composable
fun SkillSettingsScreen(
    onBack: () -> Unit,
    viewModel: SkillsViewModel = hiltViewModel(),
) {
    val row by viewModel.row.collectAsStateWithLifecycle()
    val state by viewModel.state.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    // One fetch per page visit: a body a teammate changed is re-read.
    LaunchedEffect(viewModel.skillId) { viewModel.load() }
    val skill = row ?: state.skill?.let { SkillOption(it.id, it.name, it.description, it.enabledByDefault, it.builtin) }
    val title = skill?.name.orEmpty()
    SkillsPageScaffold(title, onBack, "skill-page") {
        SkillPageContent(
            skill = skill,
            body = state.skill?.body,
            bodyLoading = state.loading,
            bodyFailed = state.loadFailed,
            onRetry = { viewModel.load(retried = true) },
        )
    }
}

/** The stateless read-only form (the prompt page's layout). */
@Composable
internal fun SkillPageContent(
    skill: SkillOption?,
    body: String?,
    bodyLoading: Boolean,
    bodyFailed: Boolean,
    onRetry: () -> Unit,
) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column {
        SectionHeader(skill?.name.orEmpty())
        SkillGroup {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(modifier = Modifier.size(36.dp), contentAlignment = Alignment.Center) {
                    Icon(ExpIcons.uiDocs, contentDescription = null, modifier = Modifier.size(20.dp), tint = muted)
                }
                Spacer(Modifier.width(12.dp))
                // Read-only text at full strength (web's readOnly inputs).
                Text(
                    skill?.name.orEmpty(),
                    modifier = Modifier.weight(1f).testTag("skill-name"),
                    style = MaterialTheme.typography.bodyMedium.copy(fontFamily = ExpMono),
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                )
            }
            GroupDivider()
            Text(
                skill?.description.orEmpty(),
                modifier = Modifier.fillMaxWidth().padding(16.dp).testTag("skill-description"),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
        Spacer(Modifier.height(8.dp))
        SkillGroup {
            when {
                // EXP-1275: the fetch = skeleton rows, never "Loading…".
                bodyLoading -> SkeletonRows(dot = false, modifier = Modifier.padding(vertical = 4.dp))
                // FEED-79: the fetch ran on open: the empty state + Retry.
                bodyFailed -> Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    modifier = Modifier.fillMaxWidth().padding(vertical = 16.dp),
                ) {
                    Text(
                        "Skill unavailable",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    TextButton(onClick = onRetry) { Text(FeedbackStates.RETRY) }
                }
                else -> Text(
                    body.orEmpty(),
                    modifier = Modifier.fillMaxWidth().padding(16.dp).testTag("skill-body"),
                    style = MaterialTheme.typography.bodySmall.copy(fontFamily = ExpMono),
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }
        }
        Spacer(Modifier.height(8.dp))
        SkillGroup {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                // Read-only: the label dims with its switch, like web's disabled row.
                Text(
                    Skills.FIELD_DEFAULT,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.5f),
                    modifier = Modifier.weight(1f),
                )
                GlassSwitch(checked = skill?.enabledByDefault == true, onCheckedChange = null, enabled = false)
            }
        }
        Spacer(Modifier.height(24.dp))
    }
}
