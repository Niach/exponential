package com.exponential.app.ui.components.picker

import androidx.compose.foundation.layout.RowScope
import androidx.compose.runtime.Composable

/**
 * EXP-1029 contract — the account picker under the shared picker API.
 *
 * The EXP-991 picker (`AccountPickerPill` + `AccountMenuItems` in
 * `ui/components`: brand mark + login email per row, the EXP-992 rate-limit
 * preview `AccountLimitBars` under the picked login) MOVES onto [Picker] in
 * EXP-1021, keeping its options and its preview: the bars ride as each row's
 * description. This file declares the typed composable over the same
 * option keys; the body renders the trigger until then.
 */
data class AccountPickerOption(
    /** The option key (`<agent>:<profile>`), `AccountOption.key` today. */
    val key: String,
    /** Contract `codingAgent`. */
    val agent: String,
    val email: String,
    /** A dead credential's health badge text, if any. */
    val healthNote: String? = null,
)

fun accountPickerItems(options: List<AccountPickerOption>): List<PickerItem<String>> =
    options.map { option ->
        PickerItem(
            value = option.key,
            label = option.email,
            description = option.healthNote,
            keywords = listOf(option.email, option.agent),
        )
    }

@Composable
fun AccountPicker(
    options: List<AccountPickerOption>,
    value: String?,
    onChange: (String) -> Unit,
    /**
     * EXP-992: the login row's own body — the brand mark, the email, its
     * health badge and the three rate-limit bars under it. The primitive
     * keeps the highlight and the click, so a login row can carry its preview
     * without inventing a second "this is picked" idiom.
     */
    renderItem: (@Composable RowScope.(item: PickerItem<String>) -> Unit)? = null,
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
        items = accountPickerItems(options),
        mode = PickerMode.Single,
        value = setOfNotNull(value),
        onChange = { picked -> picked.firstOrNull()?.let(onChange) },
        title = "Account",
        renderItem = renderItem,
        open = open,
        onOpenChange = onOpenChange,
        trigger = trigger,
    )
}
