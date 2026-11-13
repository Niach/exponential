import ExpCore
import SwiftUI

// #EXP-1804 (`sg_ci-run`, `sg_runner-settings`): the fixtures' `specimen`s
// drawn by the real views over fixture data (Android `CiSpecimens`, web
// `?dev=ci-run` / `?dev=runner-settings`). Capture only.

/// ci-run.json `specimen`: run #12 of `ci` on mint, `test (linux)` failed
/// and selected.
struct CiRunSpecimens: View {
    var body: some View {
        CiRunPage(
            run: CiRun.specimenRun,
            jobs: CiRun.specimenJobs,
            selected: CiRun.specimenSelected,
            log: CiRun.specimenLog,
            now: WireTimestamps.parse(CiRun.specimenNow) ?? Date()
        )
        .padding(.horizontal, -16)
        .accessibilityIdentifier("ci-run-specimen")
    }
}

/// runner-settings.json `specimen` + `recentJobs[0]`: mint's enabled runner,
/// owner viewing (a phone client: the phone notice shows).
struct RunnerSettingsSpecimens: View {
    var body: some View {
        RunnerSettingsView(
            caps: RunnerSettings.specimenCaps,
            platform: RunnerSettings.specimenPlatform,
            settings: RunnerSettings.specimenSettings,
            owner: true,
            recent: RunnerSettings.recentJobs(
                deviceId: RunnerSettings.specimenDeviceId,
                jobs: RunnerSettings.specimenJobs,
                now: WireTimestamps.parse(RunnerSettings.specimenNow) ?? Date()
            )
        )
        .accessibilityIdentifier("runner-settings-specimen")
    }
}
