package com.exponential.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.StepUp
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetDefaults
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.SheetPrimaryAction

/**
 * #EXP-2143 follow-up: THE "Confirm it's you" sheet (fixtures/step-up.json
 * layout, web `StepUpDialog` twin): the method's one line, its field, the
 * inline error, the other methods as text buttons, the one primary button
 * pinned at the bottom. Swipe or back dismisses (every sheet's cancel): the
 * refused action is dropped quietly and the screen under it keeps its state.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun StepUpSheet(
    state: StepUp.State,
    email: String,
    onChoose: (StepUp.Method) -> Unit,
    onSubmit: (String) -> Unit,
    onResend: () -> Unit,
    onDismiss: () -> Unit,
) {
    val method = state.method ?: return
    val busy = state.sheet == StepUp.Sheet.Busy
    var secret by rememberSaveable { mutableStateOf("") }
    // A new method starts with an empty field.
    LaunchedEffect(method) { secret = "" }
    val ready = method == StepUp.Method.Passkey || secret.isNotBlank()
    GlassSheet(
        title = StepUp.TITLE,
        onDismiss = onDismiss,
        primaryAction = SheetPrimaryAction(
            label = if (method == StepUp.Method.Passkey) StepUp.PASSKEY_BUTTON else StepUp.CONFIRM,
            onClick = { onSubmit(secret) },
            enabled = ready && !busy,
            loading = busy,
            testTag = "step-up-confirm",
        ),
    ) {
        Column(
            modifier = Modifier.padding(horizontal = GlassSheetDefaults.HorizontalPadding),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                method.description(email),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (method != StepUp.Method.Passkey) {
                GlassTextField(
                    value = secret,
                    onValueChange = { value ->
                        secret = if (method == StepUp.Method.Email) value.filter { it.isDigit() }.take(6) else value
                    },
                    singleLine = true,
                    placeholder = method.field,
                    enabled = !busy,
                    keyboardOptions = KeyboardOptions(
                        keyboardType = if (method == StepUp.Method.Email) KeyboardType.NumberPassword else KeyboardType.Password,
                    ),
                    visualTransformation = if (method == StepUp.Method.Password) {
                        PasswordVisualTransformation()
                    } else {
                        VisualTransformation.None
                    },
                    modifier = Modifier.fillMaxWidth().testTag("step-up-field"),
                )
            }
            // The refused proof: invalid input, under its field.
            FieldError(state.error)
            FlowRow(modifier = Modifier.fillMaxWidth()) {
                if (method == StepUp.Method.Email) {
                    TextButton(onClick = onResend, enabled = !busy) { Text(StepUp.RESEND) }
                }
                for (other in state.methods) {
                    if (other == method) continue
                    TextButton(
                        onClick = { onChoose(other) },
                        enabled = !busy,
                        modifier = Modifier.testTag("step-up-switch-${other.wire}"),
                    ) { Text(other.switchLabel) }
                }
            }
        }
    }
}
