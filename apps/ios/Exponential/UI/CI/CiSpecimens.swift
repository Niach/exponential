import ExpCore
import SwiftUI

// #EXP-1804 (`sg_ci-run`, `sg_runner-settings`): the fixtures' `specimen`s
// drawn by the real views over fixture data (Android `CiSpecimens`, web
// `?dev=ci-run` / `?dev=runner-settings`). Capture only.

/// ci-run.json `specimen`: run #12 of `ci` on mint, `test (linux)` failed
/// and selected.
struct CiRunSpecimens: View {
    /// #EXP-2172: the same run as a GitHub Actions run of acme/mobile whose
    /// installation accepted Actions: write (capture only).
    var github = false

    private var model: CiRunView.Model {
        guard github else { return CiRunView.specimenModel }
        let r = CiRun.specimenRun
        let run = CiRun.Run(
            id: "g1207", number: 1207, name: "build-ios", state: r.state, error: nil, sha: r.sha,
            startedAt: r.startedAt, completedAt: r.completedAt, ref: "release/2.4", provider: "github",
            externalUrl: "https://github.com/acme/mobile/actions/runs/1207", externalAccess: "write",
            title: "Bump the widget to 2.4.1", createdAt: r.createdAt
        )
        return CiRunView.Model(run: run, repository: "acme/mobile", pullRequest: nil, author: nil, jobs: CiRun.specimenJobs)
    }

    var body: some View {
        CiRunPage(
            model: model,
            selected: CiRun.specimenSelected,
            log: CiRun.specimenLog,
            now: WireTimestamps.parse(CiRun.specimenNow) ?? Date()
        )
        .padding(.horizontal, -16)
        .accessibilityIdentifier("ci-run-specimen")
    }
}

/// #EXP-2172 (`sg_ci`): ci-page.json `scenario`, the Runs tab, as a phone
/// draws it; the tabs and filters work.
struct CiPageSpecimens: View {
    @State private var state = CiPageView.State(phone: true)

    var body: some View {
        CiPagePage(input: CiPage.specimen, state: state, onState: { state = $0 })
            .padding(.horizontal, -16)
            .accessibilityIdentifier("ci-page-specimen")
    }
}

/// runner-settings.json `specimen` + `recentJobs[0]`: mint's enabled runner,
/// owner viewing (a phone client: the phone notice shows).
struct RunnerSettingsSpecimens: View {
    var body: some View {
        RunnerSettingsView(
            caps: RunnerSettings.specimenCaps,
            kind: RunnerSettings.specimenKind,
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

/// #EXP-1617 (repo-ci.json, `sg_repo-runs`): the repo page's Runs tab over
/// runRows[0] as a phone draws it (one Row per run).
struct RepoRunsSpecimens: View {
    var body: some View {
        RepoRunsTab(rows: RepoCi.runRows(
            RepoCi.specimenRuns, jobs: RepoCi.specimenJobs,
            now: WireTimestamps.parse(RepoCi.specimenNow) ?? Date()
        ))
        .accessibilityIdentifier("repo-runs-specimen")
    }
}

/// #EXP-1617 (repo-ci.json, `sg_repo-ci`): the Settings tab's CI section of a
/// GitHub repo over the same runs, a manager viewing (required `build`, two
/// secrets, Report to GitHub on, GitHub Actions on team devices off).
struct RepoCiSpecimens: View {
    @State private var settings = RepoCiSettings(
        config: RepoCiConfig(requiredChecks: RepoCi.specimenRequired, githubStatuses: true, actionsRunners: false),
        secrets: RepoCi.specimenSecrets
    )

    var body: some View {
        RepoCiSectionView(
            provider: "github",
            runs: RepoCi.specimenRuns,
            jobs: RepoCi.specimenJobs,
            settings: settings,
            manager: true,
            onSetRequired: { settings.config.requiredChecks = $0 },
            onSetSwitch: { key, on in
                switch key {
                case .githubStatuses: settings.config.githubStatuses = on
                case .actionsRunners: settings.config.actionsRunners = on
                case .pullRequestSecrets: settings.config.pullRequestSecrets = on
                }
            }
        )
        .accessibilityIdentifier("repo-ci-specimen")
    }
}
