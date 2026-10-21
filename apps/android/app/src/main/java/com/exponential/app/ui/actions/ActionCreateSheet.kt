package com.exponential.app.ui.actions

import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.SheetPrimaryAction

internal const val NEW_ACTION_TITLE = "New action"
private const val CREATE_ACTION_LABEL = "Create action"

/**
 * EXP-1296: `New action` — the saved-prompt form ([ActionPromptForm]) in a
 * full sheet, `Create action` pinned to its bottom edge (the create-board
 * sheet's shape). [onCreate] gets the form's fields; the host closes the
 * sheet when the create lands (a refusal toasts with the sheet open).
 */
@Composable
fun ActionCreateSheet(
    busy: Boolean,
    onCreate: (com.exponential.app.data.api.PromptFields) -> Unit,
    onDismiss: () -> Unit,
    form: ActionFormState = rememberActionFormState(),
) {
    GlassSheet(
        title = NEW_ACTION_TITLE,
        onDismiss = onDismiss,
        modifier = Modifier.testTag("action-create-sheet"),
        height = SheetHeight.Full,
        primaryAction = SheetPrimaryAction(
            label = CREATE_ACTION_LABEL,
            enabled = form.valid && !busy,
            loading = busy,
            onClick = { onCreate(form.fields()) },
            testTag = "action-create-submit",
        ),
    ) {
        ActionPromptForm(
            form = form,
            editable = true,
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState()),
        )
        Spacer(Modifier.height(16.dp))
    }
}
