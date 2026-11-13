package com.exponential.app.ui.ci

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.CiRun
import com.exponential.app.domain.CiRunView
import com.exponential.app.domain.RunnerSettings
import com.exponential.app.ui.components.ClaimedFailedActionToast
import com.exponential.app.ui.components.rememberTickingNow
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive

// #EXP-1617 / #EXP-1804 (fixtures/runner-settings.json, view
// `machine-settings`): a device's CI runner section as the genUI tree web
// builds ([CiRunView.runnerSectionTree], web `components/ci/runner-section.tsx`),
// as a PHONE client draws it (the phone notice always shows). One
// `ci.setRunner` per change (the whole settings object); every control
// disabled for a viewer who is not the device's owner.

/** The runner client this app is (`runnerNotice`). */
private const val CLIENT = "android"

/** The live section of [device] inside its settings sheet. */
@Composable
fun RunnerSettingsSection(
    device: SteerDevice,
    modifier: Modifier = Modifier,
    viewModel: RunnerSettingsViewModel = hiltViewModel(),
) {
    LaunchedEffect(device.rowId) { viewModel.bind(device.rowId) }
    val jobs by viewModel.jobs.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    ClaimedFailedActionToast(failure, viewModel::claimFailure)
    val stored = device.runner ?: RunnerSettings.DEFAULTS
    // The last write wins locally until the synced row catches up.
    var pending by remember(device.rowId) { mutableStateOf<RunnerSettings.Settings?>(null) }
    LaunchedEffect(device.runner) { pending = null }
    val now = rememberTickingNow(jobs.any { it.status == "in_progress" })
    RunnerSettingsView(
        caps = device.caps.orEmpty(),
        platform = device.platform,
        settings = pending ?: stored,
        owner = device.isMine,
        recent = RunnerSettings.recentJobs(device.rowId.orEmpty(), jobs, now),
        onChange = { next ->
            pending = next
            viewModel.setRunner(device.deviceId, next) { pending = null }
        },
        modifier = modifier,
    )
}

/** The stateless section ([recent] = [RunnerSettings.recentJobs]'s rows). */
@Composable
fun RunnerSettingsView(
    caps: List<String>,
    platform: String?,
    settings: RunnerSettings.Settings,
    owner: Boolean,
    recent: List<CiRun.JobRow>,
    onChange: (RunnerSettings.Settings) -> Unit,
    modifier: Modifier = Modifier,
) {
    var refused by remember { mutableStateOf(false) }
    val handle = remember { CiSurfaceHandle() }
    val tree = remember(caps, platform, settings, owner, refused, recent) {
        CiRunView.runnerSectionTree(CLIENT, caps, platform, settings, owner, refused, recent)
    }
    fun labels(values: List<String>) = JsonArray(values.map(::JsonPrimitive))
    // The ChipInput's `values` is bound: the stored labels win (a refused or
    // normalized entry is rewritten in the field).
    LaunchedEffect(settings.labels) { handle.setData(CiRunView.RUNNER_LABELS_PATH, labels(settings.labels)) }
    CiSurface(
        surfaceId = "runner-settings",
        tree = tree,
        modifier = modifier.fillMaxWidth().testTag(RunnerSettings.TestIds.SECTION),
        data = mapOf("labels" to labels(settings.labels)),
        handle = handle,
    ) { name, _, payload ->
        val (next, turnedDown) = CiRunView.runnerSettingsAfter(settings, name, payload)
        if (name == CiRunView.RunnerEvents.LABELS) {
            refused = turnedDown
            handle.setData(CiRunView.RUNNER_LABELS_PATH, labels((next ?: settings).labels))
        }
        if (next != null) onChange(next)
    }
}
