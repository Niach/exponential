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
    /** The sheet headline; the default names the picker. */
    title: String = "Issues",
    emptyText: String = "No issues",
    /**
     * EXP-892: a caller that ranks with the shared engine hands the query in
     * and the rows already ordered — the picker then renders them verbatim
     * instead of filtering a second time.
     */
    query: String? = null,
    onQueryChange: ((String) -> Unit)? = null,
    filter: Boolean = true,
    /**
     * EXP-1021: the sheet CONTROLLED by the caller, for a picker that is a
     * state machine rather than a chip (the issue screens open theirs from a
     * properties sheet that has already closed).
     */
    open: Boolean? = null,
    onOpenChange: ((Boolean) -> Unit)? = null,
    trigger: @Composable (open: () -> Unit) -> Unit = {},
) {
    Picker(
        items = issuePickerItems(issues),
        mode = mode,
        value = value,
        onChange = onChange,
        search = true,
        emptyText = emptyText,
        title = title,
        query = query,
        onQueryChange = onQueryChange,
        filter = filter,
        searchPlaceholder = "Search issues",
        open = open,
        onOpenChange = onOpenChange,
        trigger = trigger,
    )
}
