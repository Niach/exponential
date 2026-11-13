package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.RepoCiConfig
import com.exponential.app.data.api.RepoCiSecret
import com.exponential.app.data.api.RepoCiSettings
import com.exponential.app.domain.CiRun
import com.exponential.app.domain.RepoCi
import com.exponential.app.domain.RepoPage
import com.exponential.app.domain.RunnerSettings
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.ui.ci.CiRunPage
import com.exponential.app.ui.ci.RunnerSettingsView
import com.exponential.app.ui.repo.RepoCiSection
import com.exponential.app.ui.repo.RepoRunsTab
import com.exponential.app.ui.theme.AppBackground

// #EXP-1617 / #EXP-1804: the two CI specimens the styleguide lane shoots
// (no server), the same genUI trees web's `?dev=ci-run` / `?dev=runner-settings`
// paint: THE run page over ci-run.json `specimen` (shot sg_ci-run) and the
// device's CI runner section over runner-settings.json `specimen` +
// recentJobs[0] as a phone draws it (shot sg_runner-settings).

const val CiRunSpecimenTestId = "ci-run-specimen"
const val RunnerSettingsSpecimenTestId = "runner-settings-specimen"

/** ci-run.json `specimen`: run #12 of `ci` on mint, `test (linux)` selected. */
@Composable
fun CiRunSpecimenScreen() {
    var selected by remember { mutableStateOf(CiRun.SPECIMEN_SELECTED) }
    AppBackground {
        Box(Modifier.fillMaxSize().testTag(CiRunSpecimenTestId)) {
            CiRunPage(
                run = CiRun.SPECIMEN_RUN,
                jobs = CiRun.SPECIMEN_JOBS,
                selectedJobId = selected,
                log = if (selected == CiRun.SPECIMEN_SELECTED) CiRun.SPECIMEN_LOG else null,
                nowMs = WireTimestamps.parseEpochMs(CiRun.SPECIMEN_NOW) ?: 0L,
                busy = null,
                onSelectJob = { selected = it },
                onRerunFailed = {},
                onCancel = {},
                onBack = {},
            )
        }
    }
}

/** runner-settings.json `specimen`: mint's enabled runner, owner viewing. */
@Composable
fun RunnerSettingsSpecimenScreen() {
    var settings by remember { mutableStateOf(RunnerSettings.SPECIMEN_SETTINGS) }
    val now = WireTimestamps.parseEpochMs(RunnerSettings.SPECIMEN_NOW) ?: 0L
    val recent = remember { RunnerSettings.recentJobs(RunnerSettings.SPECIMEN_DEVICE_ID, RunnerSettings.SPECIMEN_JOBS, now) }
    AppBackground {
        Box(
            Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .verticalScroll(rememberScrollState())
                .padding(16.dp)
                .testTag(RunnerSettingsSpecimenTestId),
        ) {
            RunnerSettingsView(
                caps = RunnerSettings.SPECIMEN_CAPS,
                platform = RunnerSettings.SPECIMEN_PLATFORM,
                settings = settings,
                owner = true,
                recent = recent,
                onChange = { settings = it },
            )
        }
    }
}

const val RepoRunsSpecimenTestId = "repo-runs-specimen"
const val RepoCiSpecimenTestId = "repo-ci-specimen"

/** #EXP-1617 (repo-ci.json, shot sg_repo-runs): the repo page's Runs tab
 *  over runRows[0] as a phone draws it (one Row per run). */
@Composable
fun RepoRunsSpecimenScreen() {
    val now = WireTimestamps.parseEpochMs(RepoCi.SPECIMEN_NOW) ?: 0L
    val rows = remember { RepoCi.runRows(RepoCi.SPECIMEN_RUNS, RepoCi.SPECIMEN_JOBS, now) }
    AppBackground {
        Box(
            Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .verticalScroll(rememberScrollState())
                .padding(16.dp)
                .testTag(RepoRunsSpecimenTestId),
        ) {
            RepoRunsTab(rows, onOpenRun = {})
        }
    }
}

/** #EXP-1617 (repo-ci.json, shot sg_repo-ci): the Settings tab's CI section
 *  of a GitHub repo over the same runs, a manager viewing (required `build`,
 *  two secrets, Report to GitHub on). */
@Composable
fun RepoCiSpecimenScreen() {
    var ci by remember {
        mutableStateOf(
            RepoCiSettings(
                config = RepoCiConfig(requiredChecks = RepoCi.SPECIMEN_REQUIRED, githubStatuses = true, actionsRunners = false),
                secrets = RepoCi.SPECIMEN_SECRETS.map { (name, at) -> RepoCiSecret(name, at) },
            ),
        )
    }
    AppBackground {
        Box(
            Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .verticalScroll(rememberScrollState())
                .padding(16.dp)
                .testTag(RepoCiSpecimenTestId),
        ) {
            RepoCiSection(
                provider = RepoPage.PROVIDER_GITHUB,
                runs = RepoCi.SPECIMEN_RUNS,
                jobs = RepoCi.SPECIMEN_JOBS,
                ci = ci,
                canManage = true,
                onSetRequired = { ci = ci.copy(config = ci.config.copy(requiredChecks = it)) },
                onSetSwitch = { switch, on ->
                    ci = ci.copy(
                        config = when (switch) {
                            RepoCi.Switch.GithubStatuses -> ci.config.copy(githubStatuses = on)
                            RepoCi.Switch.ActionsRunners -> ci.config.copy(actionsRunners = on)
                        },
                    )
                },
                onSaveSecret = { _, _, done -> done(true) },
                onDeleteSecret = {},
            )
        }
    }
}
