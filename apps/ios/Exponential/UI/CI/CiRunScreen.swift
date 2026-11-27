import ExpCore
import ExpUI
import GRDB
import SwiftUI

// #EXP-1617 / #EXP-1804 (fixtures/ci-run.json, view `ci-run`), redesigned by
// #EXP-2172: THE run page of our CI as the genUI tree web builds
// (`CiRunView.ciRunTree`) painted by the core catalog; the phone layout on a
// compact width (`name #n` / repository in the page header, the job rows
// DRILL into the job's Card, THE round back returns), the wide one on the
// iPad. The run, its jobs, its PR and its author are synced rows; the
// selected job's LOG is read on demand (`ci.log`): once from 0, then from
// `nextOffset` every `liveTailMs` while it is not complete.

/// The route's screen: the synced run + jobs, the tailed log.
struct CiRunScreen: View {
    let runId: String
    var requestedJob: String?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.openURL) private var openURL
    @Environment(\.horizontalSizeClass) private var sizeClass

    private struct Rows: Equatable {
        var run: CiRunEntity?
        var jobs: [CiJobEntity] = []
        /// devices row id → label (the runner chip).
        var labels: [String: String] = [:]
        /// #EXP-2172: the run's pull request (number, base), when synced.
        var pullRequest: CiRun.PullRequest?
        /// The author's name (the users row of createdByUserId).
        var author: String?
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
    /// Phones: drilled into the selected job (its Card alone).
    @State private var jobOpen = false

    private var phone: Bool { sizeClass != .regular }

    private var model: CiRunView.Model? {
        rows.run.map {
            CiRunView.Model(run: $0.run, repository: repository, pullRequest: rows.pullRequest, author: rows.author, jobs: jobs)
        }
    }

    private var shownLog: String? { tail.jobId == selected && tail.loaded ? tail.text : nil }

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
            if let model {
                page(model, jobOpen: false)
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
        .modifier(CiRunHeader(model: phone ? model : nil))
        // Phones: a job row drills into the job; THE round back returns.
        .navigationDestination(isPresented: $jobOpen) {
            ZStack {
                AppBackground()
                if let model { page(model, jobOpen: true) }
            }
            .navigationTitle("")
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
            .modifier(CiRunHeader(model: model))
            .environment(\.accountId, accountId)
            .environment(\.detailColumnRoot, false)
            .bareBackButton()
        }
        .onChange(of: phone) { _, isPhone in
            if !isPhone { jobOpen = false }
        }
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

    private func page(_ model: CiRunView.Model, jobOpen open: Bool) -> some View {
        CiRunPage(
            model: model, selected: selected, log: shownLog, now: now, busy: busy, phone: phone, jobOpen: open,
            onSelect: { job in
                picked = job
                if phone { jobOpen = true }
            },
            onRerun: { rerun(failedOnly: true) },
            onRerunAll: { rerun(failedOnly: false) },
            onCancel: cancel,
            onFix: fix,
            onOpenExternal: {
                if let url = rows.run?.externalUrl.flatMap(URL.init(string:)) { openURL(url) }
            },
            onBreadcrumb: breadcrumb
        )
    }

    /// The wide breadcrumb: `ci` = the CI page, `repository` = its repo page.
    private func breadcrumb(_ value: String) {
        guard let run = rows.run else { return }
        switch value {
        case "ci": pushRoute(.ci(accountId: accountId))
        case "repository": pushRoute(.repo(accountId: accountId, teamId: run.teamId, repositoryId: run.repositoryId, isOwner: false))
        default: break
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
            let pullRequest = try run?.pullRequestId.flatMap { try PullRequestEntity.fetchOne(db, key: $0) }
                .map { CiRun.PullRequest(number: $0.number, base: $0.base) }
            let author = try run?.createdByUserId.flatMap { try UserEntity.fetchOne(db, key: $0) }?.name
            return Rows(run: run, jobs: jobs, labels: labels, pullRequest: pullRequest, author: author)
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
            log: shownLog
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

/// Phones: `name #n` over the repository in the page header (`ciRunHeader`).
private struct CiRunHeader: ViewModifier {
    let model: CiRunView.Model?

    func body(content: Content) -> some View {
        if let model {
            let header = CiRunView.ciRunHeader(model)
            content.detailHeaderTitle("\(header.title)|\(header.subtitle ?? "")") {
                VStack(alignment: .leading, spacing: 1) {
                    Text(header.title)
                        .font(.headline)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    if let subtitle = header.subtitle, !subtitle.isEmpty {
                        Text(subtitle)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }
            }
        } else {
            content
        }
    }
}

/// The stateless page (also the `ci-run` specimen). `log` = the selected
/// job's log as read so far (nil = none yet); `phone` = the compact layout,
/// `jobOpen` = drilled into the selected job.
struct CiRunPage: View {
    let model: CiRunView.Model
    let selected: String?
    let log: String?
    let now: Date
    var busy: CiRunView.Busy?
    var phone = true
    var jobOpen = false
    var onSelect: (String) -> Void = { _ in }
    var onRerun: () -> Void = {}
    var onRerunAll: () -> Void = {}
    var onCancel: () -> Void = {}
    var onFix: () -> Void = {}
    var onOpenExternal: () -> Void = {}
    var onBreadcrumb: (String) -> Void = { _ in }

    var body: some View {
        let tree = CiRunView.ciRunTree(
            model, selected: selected, log: log, now: now, phone: phone, busy: busy, jobOpen: phone && jobOpen
        )
        ScrollView {
            CiSurface(surfaceId: "ci-run-\(model.run.id)\(jobOpen ? "-job" : "")", tree: tree) { name, context, payload in
                switch name {
                case CiRunView.Events.select:
                    guard case let .string(job)? = context?["job"] else { return }
                    // Phones drill in on every press; wide, a new pick only.
                    if phone || job != selected { onSelect(job) }
                case CiRunView.Events.menu:
                    guard case let .string(value)? = payload?["value"] else { return }
                    switch CiRun.Actions.MenuItem(rawValue: value) {
                    case .rerunAll?: onRerunAll()
                    case .openExternal?: onOpenExternal()
                    case nil: break
                    }
                case CiRunView.Events.breadcrumb:
                    if case let .string(value)? = payload?["value"] { onBreadcrumb(value) }
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
