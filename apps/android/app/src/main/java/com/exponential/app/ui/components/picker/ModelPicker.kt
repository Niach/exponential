package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.vector.ImageVector

/**
 * EXP-1345 (composer-menu.json picker rule): a single-select option pick —
 * the composer's Model (and effort), the session composer's model — is THE
 * [Picker]: a phone bottom sheet of the same rows with the picked one's
 * TRAILING check, never an anchored menu with a leading check. A thin
 * wrapper over [Picker], [DevicePicker]'s shape.
 */
data class ModelPickerOption(
    val value: String,
    val label: String,
    /** The value's own glyph; null = none. */
    val icon: ImageVector? = null,
    val disabled: Boolean = false,
)

fun modelPickerItems(options: List<ModelPickerOption>): List<PickerItem<String>> =
    options.map { option ->
        PickerItem(
            value = option.value,
            label = option.label,
            icon = option.icon,
            disabled = option.disabled,
        )
    }

@Composable
fun ModelPicker(
    options: List<ModelPickerOption>,
    value: String?,
    onChange: (String) -> Unit,
    /** The sheet headline; the default names the picker. */
    title: String = "Model",
    enabled: Boolean = true,
    /** Controlled open state (a caller that tracks its open menus); null = own. */
    open: Boolean? = null,
    onOpenChange: ((Boolean) -> Unit)? = null,
    trigger: @Composable (open: () -> Unit) -> Unit = {},
) {
    Picker(
        items = modelPickerItems(options),
        mode = PickerMode.Single,
        value = setOfNotNull(value),
        onChange = { picked -> picked.firstOrNull()?.let(onChange) },
        title = title,
        enabled = enabled,
        open = open,
        onOpenChange = onOpenChange,
        trigger = trigger,
    )
}
