package com.exponential.app.ui.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.components.DeviceSetup
import com.exponential.app.ui.components.DeviceSetupViewModel
import com.exponential.app.ui.icons.ExpIcons

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

    // EXP-1554: web's step card ×4 (iOS `DevicesStepView`): the disc +
    // title header over a hairline, the setup block and the advance inside.
    OnboardingStepCard(
        icon = ExpIcons.navDevices,
        title = OnboardingCopy.DEVICES_TITLE,
        modifier = Modifier.testTag("onboarding-devices-step"),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            DeviceSetup(instanceOrigin = instanceOrigin, viewModel = viewModel)
            // A machine of one's own is what this step is for; until one shows
            // up the only honest label is "skip".
            SkipOrContinueButton(done = devices.orEmpty().isNotEmpty(), onClick = onContinue)
        }
    }
}
