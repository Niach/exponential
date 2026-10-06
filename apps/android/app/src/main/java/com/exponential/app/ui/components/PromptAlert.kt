package com.exponential.app.ui.components

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.runtime.Composable
import com.exponential.app.domain.Prompts

/**
 * EXP-1215: a [Prompts.Prompt] (copy from `prompts.json`) on the shared
 * [GlassAlert] card. The roles become the card's pills: `cancel` = a plain
 * pill, `default` = a plain pill, `primary` = the primary pill,
 * `destructive` = the destructive-tinted pill, `quietDestructive` = the
 * leading quiet text; the focus = the prompt's [Prompts.Prompt.focus]. Sites
 * pass ONLY handlers keyed by action id; an action with no handler (Cancel)
 * dismisses. [loading] = the id of the answer in flight (its pill spins, the
 * row and the dismiss paths stop answering), [disabled] = ids that cannot
 * answer yet, [testTags] = per-id test tags.
 */
@Composable
fun PromptAlert(
    prompt: Prompts.Prompt,
    onDismiss: () -> Unit,
    handlers: Map<String, () -> Unit>,
    loading: String? = null,
    disabled: Set<String> = emptySet(),
    testTags: Map<String, String> = emptyMap(),
    content: (@Composable ColumnScope.() -> Unit)? = null,
) {
    fun action(a: Prompts.Action) = GlassAlertAction(
        label = a.label,
        onClick = handlers[a.id] ?: onDismiss,
        primary = a.role == Prompts.Role.Primary,
        destructive = a.role == Prompts.Role.Destructive || a.role == Prompts.Role.QuietDestructive,
        enabled = a.id !in disabled,
        testTag = testTags[a.id],
        loading = a.id == loading,
    )
    val leading = prompt.actions.firstOrNull { it.role == Prompts.Role.QuietDestructive }
    val trailing = prompt.actions.filter { it !== leading }
    GlassAlert(
        title = prompt.title,
        body = prompt.body,
        onDismiss = onDismiss,
        leading = leading?.let(::action),
        trailing = trailing.map(::action),
        defaultAction = trailing.indexOfFirst { it.id == prompt.focus }.takeIf { it >= 0 },
        content = content,
    )
}
