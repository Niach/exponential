package com.exponential.app.ui.instance

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import com.exponential.app.AppConstants

@Composable
fun InstanceScreen(
    onContinue: (String) -> Unit,
    showCancel: Boolean = false,
    onCancel: (() -> Unit)? = null,
    // The cloud preset is hidden when a cloud account already exists in the
    // AccountStore. Re-activating it from the add-server flow re-runs
    // upsertAndActivate which races SyncManager's DB swap — easier to remove
    // the path entirely. Users can still switch back to the existing cloud
    // account from Settings.
    cloudAlreadyAdded: Boolean = false,
) {
    var input by remember { mutableStateOf(TextFieldValue("https://")) }
    val canSubmit = input.text.length > 8

    Column(
        modifier = Modifier
            .fillMaxSize()
            .windowInsetsPadding(WindowInsets.systemBars)
            .padding(24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.Start,
    ) {
        Text(
            "Connect to Exponential",
            style = MaterialTheme.typography.titleLarge,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Spacer(Modifier.height(24.dp))

        if (!cloudAlreadyAdded) {
            Button(
                onClick = { onContinue(AppConstants.PUBLIC_CLOUD_URL) },
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Use Exponential Cloud")
            }

            Spacer(Modifier.height(16.dp))

            Row(verticalAlignment = Alignment.CenterVertically) {
                HorizontalDivider(modifier = Modifier.weight(1f))
                Text(
                    "or self-host",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 8.dp),
                )
                HorizontalDivider(modifier = Modifier.weight(1f))
            }

            Spacer(Modifier.height(16.dp))
        }

        Text(
            "Self-hosted? Enter the full URL of your server.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(8.dp))

        OutlinedTextField(
            value = input,
            onValueChange = { input = it },
            singleLine = true,
            placeholder = { Text("https://exp.example.com") },
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
            modifier = Modifier.fillMaxWidth(),
        )

        Spacer(Modifier.height(16.dp))

        Button(
            onClick = { if (canSubmit) onContinue(input.text) },
            enabled = canSubmit,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Continue")
        }

        if (showCancel && onCancel != null) {
            Spacer(Modifier.height(8.dp))
            TextButton(onClick = onCancel, modifier = Modifier.fillMaxWidth()) {
                Text("Cancel")
            }
        }
    }
}
