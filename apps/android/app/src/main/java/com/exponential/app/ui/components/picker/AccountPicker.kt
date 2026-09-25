package com.exponential.app.ui.components.picker

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
    trigger: @Composable (open: () -> Unit) -> Unit,
) {
    Picker(
        items = accountPickerItems(options),
        mode = PickerMode.Single,
        value = setOfNotNull(value),
        onChange = { picked -> picked.firstOrNull()?.let(onChange) },
        title = "Account",
        trigger = trigger,
    )
}
