package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.AgentUsageWindow
import com.exponential.app.data.api.DeviceLatestVersions
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.AgentHealth
import com.exponential.app.domain.AgentProfileUsageRow
import com.exponential.app.domain.DevicesList
import com.exponential.app.ui.session.DeviceCard
import com.exponential.app.ui.session.DeviceGrid
import com.exponential.app.ui.session.OpenAccountMemory
import com.exponential.app.ui.theme.AppBackground
import java.time.Duration
import java.time.Instant

// #EXP-1643 (fixtures/devices-list.json): the Android twin of the styleguide
// entry `device-attention`, line for line: two REAL device cards, `macbook`
// outdated with THE account panel open on its badged login, `mint` closed.

const val DeviceAttentionSpecimenTestId = "device-attention-specimen"

@Composable
fun DeviceAttentionSpecimenScreen() {
    AppBackground {
        BoxWithConstraints(
            Modifier.fillMaxSize().statusBarsPadding().testTag(DeviceAttentionSpecimenTestId),
        ) {
            val inset = 12.dp
            val columns = DevicesList.columns((maxWidth - inset * 2).value.toInt())
            val devices = remember { listOf(device("macbook", "0.14.78"), device("mint", "0.14.79")) }
            Box(Modifier.padding(inset)) {
                DeviceGrid(
                    devices = devices,
                    columns = columns,
                    key = { it.deviceId },
                    card = { device ->
                        val rows = remember(device.deviceId) { logins(device) }
                        val open = remember(device.deviceId) {
                            OpenAccountMemory(
                                if (device.deviceId == "macbook") {
                                    DevicesList.RememberedAccount.Login(rows[1].key)
                                } else {
                                    DevicesList.RememberedAccount.Untouched
                                },
                            )
                        }
                        DeviceCard(
                            device = device,
                            latestVersions = DeviceLatestVersions(desktop = "0.14.79"),
                            commandStates = emptyMap(),
                            deviceBusy = false,
                            onOpenSettings = {},
                            logins = rows,
                            onRemoveAccount = {},
                            onSignOutAccount = {},
                            onSignIn = { _, _ -> },
                            onAddAccount = {},
                            onUpdate = {},
                            openAccount = open,
                        )
                    },
                )
            }
        }
    }
}

private fun device(name: String, version: String) = SteerDevice(
    deviceId = name,
    deviceLabel = name,
    agents = listOf("claude", "codex"),
    acpAgents = listOf("claude", "codex"),
    registered = true,
    version = version,
    agentAccounts = emptyMap(),
)

private fun logins(device: SteerDevice): List<AgentProfileUsageRow> {
    val now = Instant.now()
    fun reset(hours: Long, minutes: Long) = now.plus(Duration.ofHours(hours).plusMinutes(minutes).plusSeconds(30)).toString()
    val usage = AgentUsage(
        fetchedAt = now.toString(),
        windows = listOf(
            AgentUsageWindow("session", "Current session", 42.0, reset(2, 10)),
            AgentUsageWindow("weekly", "All models", 81.0, reset(86, 0)),
            AgentUsageWindow("model:fable", "Fable", 64.0, reset(86, 0)),
        ),
    )
    fun row(agent: String, profileId: String, email: String, health: AgentHealth) = AgentProfileUsageRow(
        key = "${device.deviceId}:$agent:$profileId",
        deviceId = device.deviceId,
        deviceLabel = device.deviceLabel,
        mine = true,
        online = true,
        agent = agent,
        profileId = profileId,
        active = false,
        signedIn = true,
        health = health,
        email = email,
        plan = "Max",
        usage = usage,
        checkedAt = null,
    )
    return listOf(
        row("claude", "work", "work@yourev.at", AgentHealth.Ok),
        row("claude", "danny", "alex@acme.example", AgentHealth.NeedsRelogin),
        row("codex", "danny", "alex@acme.example", AgentHealth.Ok),
    )
}
