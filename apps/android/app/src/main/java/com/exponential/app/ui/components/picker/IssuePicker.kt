package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector

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
    /**
     * EXP-1021: the row's leading glyph and its tone — the issue's STATUS, the
     * way the relations linker has always drawn it, resolved at the call site
     * (`StatusIcon`'s own pair) because an issue row has no icon of its own on
     * the wire. Web, iOS and the IDE carry the same two fields. The LABEL
     * stays `IDENT Title` on one line: that is the ×4 contract and the
     * fixture, and only the glyph comes back.
     */
    val icon: ImageVector? = null,
    val color: Color? = null,
)

fun issuePickerItems(issues: List<IssuePickerIssue>): List<PickerItem<String>> =
    issues.map { issue ->
        PickerItem(
            value = issue.id,
            label = "${issue.identifier} ${issue.title}",
            icon = issue.icon,
            color = issue.color,
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
