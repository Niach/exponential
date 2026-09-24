package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable

/**
 * EXP-1029 contract — the issue picker (single or multi): rows read
 * `IDENT Title`, search matches the identifier and the title. The composer
 * picks several (a batch); relations, duplicates and the stack dialog pick
 * one. Search over a big board runs the ONE engine (EXP-892) at the call
 * site; the picker renders what it is handed.
 */
data class IssuePickerIssue(
    val id: String,
    val identifier: String,
    val title: String,
    val disabled: Boolean = false,
)

fun issuePickerItems(issues: List<IssuePickerIssue>): List<PickerItem<String>> =
    issues.map { issue ->
        PickerItem(
            value = issue.id,
            label = "${issue.identifier} ${issue.title}",
            disabled = issue.disabled,
            keywords = listOf(issue.identifier, issue.title),
        )
    }

@Composable
fun IssuePicker(
    issues: List<IssuePickerIssue>,
    value: Set<String>,
    onChange: (Set<String>) -> Unit,
    mode: PickerMode = PickerMode.Single,
    trigger: @Composable (open: () -> Unit) -> Unit,
) {
    Picker(
        items = issuePickerItems(issues),
        mode = mode,
        value = value,
        onChange = onChange,
        search = true,
        emptyText = "No issues",
        title = "Issues",
        trigger = trigger,
    )
}
