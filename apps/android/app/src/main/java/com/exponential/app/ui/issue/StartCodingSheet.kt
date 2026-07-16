package com.exponential.app.ui.issue

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.theme.TextEmphasis

// The remote Start-coding options sheet (EXP-149) — the Android twin of the
// desktop IDE's Start-coding dialog (single-issue mode): Model / Effort chips
// over the domain-contract value lists, ultracode switch (it IS
// `--effort ultracode`, so it disables the Effort chips), plan-mode switch
// (default OFF — the session runs on an unattended desktop), plus a desktop
// picker when more than one is online. Last-used options persist via
// SharedPreferences (web/iOS parity); stored values are validated against the
// contract on read so a stale entry can never send a value the server rejects.

/** Sentinel-free UI state: an empty effort means "CLI default" (omit --effort). */
private const val CLI_DEFAULT_EFFORT = ""

private const val PREFS_NAME = "coding_start"

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun StartCodingSheet(
    devices: List<SteerDevice>,
    onStart: (SteerDevice, SteerStartOptions) -> Unit,
    onDismiss: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE) }
    var model by remember {
        mutableStateOf(
            prefs.getString("model", null)
                ?.takeIf { it in DomainContract.codingModelValues }
                ?: DomainContract.codingModelValues.first(),
        )
    }
    var effort by remember {
        mutableStateOf(
            prefs.getString("effort", null)
                ?.takeIf { it == CLI_DEFAULT_EFFORT || it in DomainContract.codingEffortValues }
                ?: CLI_DEFAULT_EFFORT,
        )
    }
    var ultracode by remember { mutableStateOf(prefs.getBoolean("ultracode", false)) }
    var planMode by remember { mutableStateOf(prefs.getBoolean("planMode", false)) }
    var deviceId by remember { mutableStateOf(devices.firstOrNull()?.deviceId) }

    val device = devices.firstOrNull { it.deviceId == deviceId } ?: devices.firstOrNull()

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = { BottomSheetDefaults.DragHandle() },
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(bottom = 24.dp),
        ) {
            Text(
                text = "Start coding",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp),
            )

            if (devices.size > 1) {
                SectionLabel("Desktop")
                devices.forEach { candidate ->
                    ListItem(
                        headlineContent = {
                            Text(candidate.deviceLabel.ifBlank { candidate.deviceId })
                        },
                        leadingContent = {
                            Icon(Icons.Filled.Computer, contentDescription = null)
                        },
                        trailingContent = if (candidate.deviceId == device?.deviceId) {
                            { Icon(Icons.Filled.Check, contentDescription = "Selected") }
                        } else null,
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { deviceId = candidate.deviceId },
                    )
                }
                Spacer(Modifier.height(8.dp))
            }

            SectionLabel("Model")
            FlowRow(
                modifier = Modifier.padding(horizontal = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                DomainContract.codingModelValues.forEach { value ->
                    FilterChip(
                        selected = model == value,
                        onClick = { model = value },
                        label = { Text(modelLabel(value)) },
                    )
                }
            }

            Spacer(Modifier.height(8.dp))
            SectionLabel("Effort")
            FlowRow(
                modifier = Modifier.padding(horizontal = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                (listOf(CLI_DEFAULT_EFFORT) + DomainContract.codingEffortValues).forEach { value ->
                    FilterChip(
                        selected = effort == value,
                        onClick = { effort = value },
                        label = { Text(effortLabel(value)) },
                        enabled = !ultracode,
                    )
                }
            }

            Spacer(Modifier.height(8.dp))
            SwitchRow(
                title = "Ultracode",
                subtitle = "Dynamic multi-agent workflows — overrides the effort level.",
                checked = ultracode,
                onCheckedChange = { ultracode = it },
            )
            SwitchRow(
                title = "Plan mode",
                subtitle = "Starts with a plan that needs approval — from the web or at the desktop.",
                checked = planMode,
                onCheckedChange = { planMode = it },
            )

            Spacer(Modifier.height(12.dp))
            Button(
                onClick = {
                    val target = device ?: return@Button
                    prefs.edit()
                        .putString("model", model)
                        .putString("effort", effort)
                        .putBoolean("ultracode", ultracode)
                        .putBoolean("planMode", planMode)
                        .apply()
                    onStart(
                        target,
                        SteerStartOptions(
                            model = model,
                            effort = effort,
                            ultracode = ultracode,
                            planMode = planMode,
                        ),
                    )
                    onDismiss()
                },
                enabled = device != null,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 24.dp),
            ) {
                Text(
                    if (device != null && devices.size == 1) {
                        "Start coding on ${device.deviceLabel.ifBlank { device.deviceId }}"
                    } else {
                        "Start coding"
                    },
                )
            }
        }
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(
        text = text,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        modifier = Modifier.padding(horizontal = 24.dp, vertical = 4.dp),
    )
}

@Composable
private fun SwitchRow(
    title: String,
    subtitle: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 24.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(
                subtitle,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

private fun modelLabel(value: String): String =
    value.replaceFirstChar { it.uppercaseChar() }

private fun effortLabel(value: String): String = when (value) {
    CLI_DEFAULT_EFFORT -> "CLI default"
    "xhigh" -> "XHigh"
    else -> value.replaceFirstChar { it.uppercaseChar() }
}
