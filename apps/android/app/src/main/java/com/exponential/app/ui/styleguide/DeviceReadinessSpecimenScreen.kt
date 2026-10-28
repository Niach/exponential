package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DeviceReadiness
import com.exponential.app.domain.parseDeviceDoctor
import com.exponential.app.ui.components.DeviceReadinessBlock
import com.exponential.app.ui.components.DeviceReadinessRow
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis

// EXP-1375 (fixtures/device-doctor.json): the Android twin of the styleguide
// entry `device-readiness`, its low-disk case (the fixture's last) as a
// phone draws them (always another device): the block in Device settings,
// then the composer's low-disk row (`DeviceReadiness.warningRow`, a warning
// with no pill). Capture-only (shot `sg_device-readiness`,
// StyleguideScreenshotsTest#captureOnlyScreens).

const val DeviceReadinessSpecimenTestId = "device-readiness-specimen"

private val cases = listOf(
    "EXP-1375: low disk with two kept worktrees: a warning, never a blocker" to
        """{"checkedAt":"2026-10-28T09:00:00.000Z","items":[{"key":"git","group":"required","state":"ok","detail":"2.55.0"},{"key":"claude","group":"agents","state":"ok","detail":"2.1.289"},{"key":"codex","group":"agents","state":"missing","detail":"Signed out","action":"sign_in"},{"key":"computer_use","group":"computer_use","state":"off"},{"key":"disk","group":"storage","state":"action","detail":"6.2 GB free"},{"key":"worktree-1","group":"storage","parent":"disk","label":"exp-session-9497089a","state":"off","detail":"Uncommitted changes"},{"key":"worktree-2","group":"storage","parent":"disk","label":"exp-EXP-1375","state":"off","detail":"Unpushed commits"}]}""",
)

@Composable
fun DeviceReadinessSpecimenScreen() {
    AppBackground {
        Box(Modifier.fillMaxSize().statusBarsPadding().testTag(DeviceReadinessSpecimenTestId)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                for ((name, json) in cases) {
                    val doctor = parseDeviceDoctor(json) ?: continue
                    Caption(name)
                    DeviceReadinessBlock(
                        groups = DeviceReadiness.groups(doctor, remote = true),
                        onAction = {},
                        onComputerUseChange = null,
                    )
                    DeviceReadiness.warningRow(doctor, remote = true)?.let { row ->
                        Caption("Composer")
                        DeviceReadinessRow(row = row, onAction = {}, horizontalPadding = 0.dp, verticalPadding = 4.dp)
                    }
                }
            }
        }
    }
}

@Composable
private fun Caption(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier.padding(top = 10.dp),
    )
}
