package com.exponential.app.ui.onboarding

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.components.DeviceSetup
import com.exponential.app.ui.components.DeviceSetupViewModel
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.theme.TextEmphasis

/**
 * "Set up your devices" (EXP-725): the [OnboardingCopy] header, the shared
 * [DeviceSetup] block, and the Skip/Continue button. One block, four hosts
 * (EXP-1169); this wrapper is two of them:
 *  - wizard step 4 on the create path, the LAST step because finishing it
 *    means walking over to another machine;
 *  - the join step every invite-accept surface (the wizard's team step, the
 *    invite link screen, the zero-team sheet) shows a caller who owns no
 *    machine yet ([com.exponential.app.ui.components.needsJoinDeviceStep]).
 *
 * Skippable: the trailing button reads [OnboardingCopy.SKIP] until a machine
 * of the user's own has registered, then [OnboardingCopy.CONTINUE].
 */
@Composable
fun OnboardingDevicesStep(
    instanceOrigin: String?,
    onContinue: () -> Unit,
    viewModel: DeviceSetupViewModel = hiltViewModel(),
) {
    val devices by viewModel.devices.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier
            .widthIn(max = 460.dp)
            .fillMaxWidth()
            .testTag("onboarding-devices-step"),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            OnboardingCopy.DEVICES_TITLE,
            style = MaterialTheme.typography.headlineSmall,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(12.dp))
        Text(
            OnboardingCopy.DEVICES_SUBTITLE,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(28.dp))

        DeviceSetup(instanceOrigin = instanceOrigin, viewModel = viewModel)

        Spacer(Modifier.height(28.dp))
        // A machine of one's own is what this step is for; until one shows up
        // the only honest label is "skip".
        GlassSubmitButton(
            label = if (devices.orEmpty().isEmpty()) OnboardingCopy.SKIP else OnboardingCopy.CONTINUE,
            onClick = onContinue,
        )
    }
}
