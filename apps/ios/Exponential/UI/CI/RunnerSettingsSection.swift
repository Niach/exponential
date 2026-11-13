import ExpCore
import ExpUI
import GRDB
import SwiftUI

// #EXP-1617 / #EXP-1804 (fixtures/runner-settings.json, view
// `machine-settings`): a device's CI runner section as the genUI tree web
// builds (`CiRunView.runnerSectionTree`, web `components/ci/runner-section.tsx`),
// as a PHONE client draws it (the phone notice always shows). One
// `ci.setRunner` per change (the whole settings object); every control
// disabled for a viewer who is not the device's owner.

/// The runner client this app is (`runnerNotice`).
private let runnerClient = "ios"

/// The live section of `device` inside its settings sheet.
struct RunnerSettingsSection: View {
    let device: SteerDevice

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster

    /// The last write wins locally until the synced row catches up.
    @State private var pending: RunnerSettings.Settings?
    /// repository id → `owner/name`, filled per team on demand.
    @State private var repoNames: [String: String] = [:]
    @State private var askedTeams: Set<String> = []
    @State private var now = Date()

    private struct Row: Equatable {
        let job: CiJobEntity
        let run: CiRunEntity?
    }

    /// Rows read before `recentJobs` sorts and caps them.
    private static let recentWindow = 25

    @State private var rows: [Row] = []

    var body: some View {
        let recentJobs = rows.map { row in
            RunnerSettings.Job(
                id: row.job.id, runnerDeviceId: row.job.runnerDeviceId, check: row.job.check,
                runNumber: row.run?.number ?? 0, repo: row.run.flatMap { repoNames[$0.repositoryId] } ?? "",
                status: row.job.status, conclusion: row.job.conclusion,
                startedAt: row.job.startedAt, completedAt: row.job.completedAt
            )
        }
        RunnerSettingsView(
            caps: device.caps ?? [],
            platform: device.platform,
            settings: pending ?? device.runner,
            owner: device.isMine,
            recent: RunnerSettings.recentJobs(deviceId: device.rowId ?? "", jobs: recentJobs, now: now),
            onChange: save
        )
        .onChange(of: device.runner) { _, _ in pending = nil }
        .task(id: device.rowId) { await observe() }
        .task(id: rows.contains { $0.job.status == "in_progress" }) {
            while rows.contains(where: { $0.job.status == "in_progress" }), !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                now = Date()
            }
        }
    }

    private func observe() async {
        guard let rowId = device.rowId, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let window = Self.recentWindow
        let values = ValueObservation.tracking { db -> [Row] in
            let jobs = try CiJobEntity
                .filter(Column("runner_device_id") == rowId)
                .order(Column("started_at").desc)
                .limit(window)
                .fetchAll(db)
            let runs = try CiRunEntity.filter(keys: Array(Set(jobs.map(\.runId)))).fetchAll(db)
            let byId = Dictionary(runs.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            return jobs.map { Row(job: $0, run: byId[$0.runId]) }
        }
        do {
            for try await next in values.values(in: pool) {
                rows = next
                loadRepos(Set(next.compactMap { $0.run?.teamId }))
            }
        } catch {}
    }

    private func loadRepos(_ teamIds: Set<String>) {
        for teamId in teamIds where !askedTeams.contains(teamId) {
            askedTeams.insert(teamId)
            Task {
                do {
                    let repos = try await deps.repositoriesApi.list(accountId: accountId, teamId: teamId)
                    for repo in repos { repoNames[repo.id] = repo.fullName }
                } catch {
                    askedTeams.remove(teamId)
                }
            }
        }
    }

    /// `ci.setRunner` with the WHOLE object; a failure toasts and hands the
    /// stored value back.
    private func save(_ next: RunnerSettings.Settings) {
        pending = next
        Task {
            do {
                try await deps.ciApi.setRunner(accountId: accountId, deviceId: device.deviceId, settings: next)
            } catch {
                pending = nil
                toaster.error(FeedbackStates.Errors.title(verb: "save", thing: "the CI runner"), description: error.userFacingMessage)
            }
        }
    }
}

/// The stateless section (`recent` = `RunnerSettings.recentJobs`'s rows); also
/// the `runner-settings` specimen.
struct RunnerSettingsView: View {
    let caps: [String]
    let platform: String?
    let settings: RunnerSettings.Settings
    let owner: Bool
    let recent: [CiRun.JobRow]
    var client = runnerClient
    var onChange: (RunnerSettings.Settings) -> Void = { _ in }

    @State private var refused = false
    @State private var handle = CiSurfaceHandle()

    private static func labels(_ values: [String]) -> CardJSON { .array(values.map(CardJSON.string)) }

    var body: some View {
        let tree = CiRunView.runnerSectionTree(
            client: client, caps: caps, platform: platform, settings: settings, owner: owner, refused: refused, recent: recent
        )
        CiSurface(
            surfaceId: "runner-settings",
            tree: tree,
            data: ["labels": Self.labels(settings.labels)],
            handle: handle
        ) { name, _, payload in
            let (next, turnedDown) = CiRunView.runnerSettingsAfter(settings, event: name, payload: payload)
            if name == CiRunView.RunnerEvents.labels {
                refused = turnedDown
                // The stored labels win: a refused or normalized entry is
                // rewritten in the field.
                handle.setData(CiRunView.runnerLabelsPath, Self.labels((next ?? settings).labels))
            }
            if let next { onChange(next) }
        }
        .onChange(of: settings.labels) { _, labels in
            handle.setData(CiRunView.runnerLabelsPath, Self.labels(labels))
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(RunnerSettings.TestIds.section)
    }
}
