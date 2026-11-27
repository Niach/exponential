import ExpCore
import ExpUI
import GRDB
import SwiftUI

// #EXP-1617 / #EXP-1804 (fixtures/ci-run.json, view `ci-run`): THE run page
// of our own CI, the PHONE layout, as the genUI tree web builds
// (`CiRunView.ciRunTree`) painted by the core catalog. The run and its jobs
// are synced rows; the selected job's LOG is read on demand (`ci.log`): once
// from 0, then from `nextOffset` every `liveTailMs` while it is not complete.

/// The route's screen: the synced run + jobs, the tailed log.
struct CiRunScreen: View {
    let runId: String
    var requestedJob: String?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.openURL) private var openURL

    private struct Rows: Equatable {
        var run: CiRunEntity?
        var jobs: [CiJobEntity] = []
        /// devices row id → label (the runner tile).
        var labels: [String: String] = [:]
        /// #EXP-2172: the run's pull request number, when synced.
        var pullRequestNumber: Int?
    }

    @State private var rows = Rows()
    @State private var resolved = false
    /// The person's pick (`?job=`, then opening a job); nil = the rule's pick.
    @State private var picked: String?
    @State private var tail = CiRun.LogTail()
    @State private var busy: CiRunView.Busy?
    @State private var now = Date()
    /// The run's repository full name (`repositories.list` of its team).
    @State private var repository: String?

    private var jobs: [CiRun.Job] {
        rows.jobs.map { $0.job(runnerLabel: $0.runnerDeviceId.flatMap { rows.labels[$0] }) }
    }

    private var selected: String? { CiRun.defaultJob(jobs, requested: picked ?? requestedJob) }

    private var running: Bool {
        rows.run?.state == "pending" || rows.jobs.contains { $0.status == "in_progress" }
    }

    var body: some View {
        ZStack {
            AppBackground()
            if let run = rows.run {
                CiRunPage(
                    run: run.run, jobs: jobs, selected: selected,
                    log: tail.jobId == selected && tail.loaded ? tail.text : nil,
                    now: now, busy: busy,
                    repository: repository, pullRequestNumber: rows.pullRequestNumber,
                    onSelect: { picked = $0 },
                    onRerun: { rerun(failedOnly: true) },
                    onRerunAll: { rerun(failedOnly: false) },
                    onCancel: cancel,
                    onFix: fix,
                    onOpenExternal: {
                        if let url = rows.run?.externalUrl.flatMap(URL.init(string:)) { openURL(url) }
                    }
                )
            } else if resolved {
                EmptyState(title: "Not found", description: "It may still be syncing, or it was deleted.", icon: AppIcons.uiDocs)
                    .frame(maxHeight: .infinity)
            } else {
                SkeletonRows(rows: 4)
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .navigationTitle("")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task { await observe() }
        .task(id: rows.run?.teamId) { await loadRepository() }
        .task(id: selected) { await tailLog(selected) }
        .task(id: running) {
            // The durations of what still runs tick.
            while running, !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                now = Date()
            }
        }
    }

    private func observe() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else {
            resolved = true
            return
        }
        let id = runId
        let values = ValueObservation.tracking { db -> Rows in
            let run = try CiRunEntity.fetchOne(db, key: id)
            let jobs = try CiJobEntity.filter(Column("run_id") == id).order(Column("position")).fetchAll(db)
            let deviceIds = Set(jobs.compactMap(\.runnerDeviceId))
            var labels: [String: String] = [:]
            if !deviceIds.isEmpty {
                for device in try DeviceEntity.filter(keys: Array(deviceIds)).fetchAll(db) {
                    labels[device.id] = device.label.isEmpty ? device.deviceId : device.label
                }
            }
            let number = try run?.pullRequestId.flatMap { try PullRequestEntity.fetchOne(db, key: $0) }?.number
            return Rows(run: run, jobs: jobs, labels: labels, pullRequestNumber: number)
        }
        do {
            for try await next in values.values(in: pool) {
                rows = next
                resolved = true
            }
        } catch {
            resolved = true
        }
    }

    /// `ci.log` from 0, then from `nextOffset` every `liveTailMs` until whole.
    private func tailLog(_ jobId: String?) async {
        tail = CiRun.LogTail(jobId: jobId)
        guard let jobId else { return }
        while !Task.isCancelled {
            let from = tail.offset
            if let chunk = try? await deps.ciApi.log(accountId: accountId, jobId: jobId, offset: from) {
                tail = tail.appending(jobId: jobId, readFrom: from, text: chunk.text, nextOffset: chunk.nextOffset, complete: chunk.complete)
                if chunk.complete { return }
            }
            try? await Task.sleep(for: .milliseconds(CiRun.liveTailMs))
        }
    }

    private func loadRepository() async {
        guard let run = rows.run else { return }
        let repos = (try? await deps.repositoriesApi.list(accountId: accountId, teamId: run.teamId)) ?? []
        repository = repos.first { $0.id == run.repositoryId }?.fullName
    }

    /// Ours: a new run opens; GitHub: the same run, a new attempt.
    private func rerun(failedOnly: Bool) {
        guard busy == nil else { return }
        busy = failedOnly ? .rerun : .rerunAll
        Task {
            do {
                let next = try await deps.ciApi.rerun(accountId: accountId, runId: runId, failedOnly: failedOnly)
                busy = nil
                if next != runId { pushRoute(.ciRun(accountId: accountId, runId: next)) }
            } catch {
                busy = nil
                toaster.error(
                    FeedbackStates.Errors.title(verb: "re-run", thing: failedOnly ? "the failed jobs" : "the run"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    /// #EXP-2172: THE new-chat composer on the run's repository, its text
    /// the failing job and the end of its log (ci-run.json `fixPrompt`).
    private func fix() {
        guard let run = rows.run else { return }
        let job = jobs.first { $0.id == selected }
        let text = CiRun.fixPrompt(
            run: run.run, repository: repository ?? "", job: job?.check,
            log: tail.jobId == selected && tail.loaded ? tail.text : nil
        )
        pushRoute(.agent(accountId: accountId, seed: AgentComposerSeed(text: text, teamId: run.teamId, repositoryId: run.repositoryId)))
    }

    private func cancel() {
        guard busy == nil else { return }
        busy = .cancel
        Task {
            do {
                try await deps.ciApi.cancel(accountId: accountId, runId: runId)
            } catch {
                toaster.error(FeedbackStates.Errors.title(verb: "cancel", thing: "the run"), description: error.userFacingMessage)
            }
            busy = nil
        }
    }
}

/// The stateless page (also the `ci-run` specimen). `log` = the selected
/// job's log as read so far (nil = none yet).
struct CiRunPage: View {
    let run: CiRun.Run
    let jobs: [CiRun.Job]
    let selected: String?
    let log: String?
    let now: Date
    var busy: CiRunView.Busy?
    var repository: String?
    var pullRequestNumber: Int?
    var onSelect: (String) -> Void = { _ in }
    var onRerun: () -> Void = {}
    var onRerunAll: () -> Void = {}
    var onCancel: () -> Void = {}
    var onFix: () -> Void = {}
    var onOpenExternal: () -> Void = {}

    var body: some View {
        let tree = CiRunView.ciRunTree(
            run: run, jobs: jobs, selected: selected, log: log, now: now, phone: true, busy: busy,
            repository: repository, pullRequestNumber: pullRequestNumber
        )
        ScrollView {
            CiSurface(surfaceId: "ci-run-\(run.id)", tree: tree) { name, context, payload in
                switch name {
                case CiRunView.Events.select:
                    guard case let .string(job)? = context?["job"] else { return }
                    // A Collapsible: closing the open one keeps the selection.
                    if case .bool(false)? = payload?["open"] { return }
                    if job != selected { onSelect(job) }
                case CiRunView.Events.rerun: onRerun()
                case CiRunView.Events.rerunAll: onRerunAll()
                case CiRunView.Events.cancel: onCancel()
                case CiRunView.Events.fix: onFix()
                case CiRunView.Events.openExternal: onOpenExternal()
                default: break
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 24)
        }
        .accessibilityIdentifier(CiRun.TestIds.page)
    }
}
