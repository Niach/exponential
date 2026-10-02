package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle

/**
 * "Add device" (EXP-1169): the [DeviceSetup] block in the shared sheet shell,
 * no trailing button. Opened by the Devices tab's "My devices" pill and by the
 * coding readiness sheet's "Set up a server" fix.
 */
@Composable
fun AddDeviceSheet(
    onDismiss: () -> Unit,
    viewModel: DeviceSetupViewModel = hiltViewModel(),
) {
    val instanceOrigin by viewModel.instanceOrigin.collectAsStateWithLifecycle()
    GlassSheet(title = "Add device", onDismiss = onDismiss) {
        // GlassSheet bounds its content slot but never scrolls it: the caller
        // owns the scroller.
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = GlassSheetDefaults.HorizontalPadding)
                .testTag("add-device-sheet"),
        ) {
            DeviceSetup(instanceOrigin = instanceOrigin, viewModel = viewModel)
        }
    }
}
