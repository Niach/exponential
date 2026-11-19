import ExpCore
import ExpUI
import GRDB
import SwiftUI

// #EXP-1617 E4 part 4 (fixtures/repo-ci.json, views `repo-runs` / `repo-ci`):
// the repo page's Runs tab and the CI section of its Settings tab, phone
// layout, as the genUI trees web builds (`RepoCiView`, web
// `lib/repo-ci-view.ts`, Android `RepoCiSection.kt`), painted by `CiSurface`
// like the run page and the runner section. The two overlays stay E3's:
// Delete's AlertDialog = `GlassAlert`, Add secret's phone Drawer = THE sheet
// (`AddSecretSheet`).

/// RUNS: `runRows` newest first (one Row per run); none = the EmptyState.
struct RepoRunsTab: View {
    let rows: [RepoCi.RunRow]
    var onOpenRun: (String) -> Void = { _ in }

    var body: some View {
        if rows.isEmpty {
            EmptyState(title: RepoPage.Copy.Runs.empty, size: .list)
        } else {
            CiSurface(surfaceId: "repo-runs", tree: RepoCiView.runsTree(rows)) { name, context, _ in
                if name == RepoCiView.Events.openRun, case let .string(runId)? = context?["run"] { onOpenRun(runId) }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(RepoCi.TestIds.runs)
        }
    }
}

/// SETTINGS › CI (`RepoCiView.sectionTree`). `settings` nil = still loading
/// (the secrets block waits). `manager` false = every control disabled, the
/// secret actions hidden.
struct RepoCiSectionView: View {
    let provider: String
    let runs: [RepoCi.Run]
    let jobs: [RepoCi.Job]
    let settings: RepoCiSettings?
    let manager: Bool
    var onSetRequired: ([String]) -> Void = { _ in }
    var onSetSwitch: (RepoCi.Switch, Bool) -> Void = { _, _ in }
    /// Throws = the sheet stays open (the caller toasts why).
    var onSaveSecret: @MainActor (String, String) async throws -> Void = { _, _ in }
    var onDeleteSecret: (String) -> Void = { _ in }

    @State private var handle = CiSurfaceHandle()
    @State private var deleting: DeleteTarget?
    @State private var adding = false

    private struct DeleteTarget: Identifiable {
        let name: String
        var id: String { name }
    }

    private static func values(_ list: [String]) -> CardJSON { .array(list.map(CardJSON.string)) }

    var body: some View {
        let config = settings?.config ?? RepoCiConfig()
        let tree = RepoCiView.sectionTree(
            provider: provider,
            pipelines: RepoCi.pipelineRows(runs),
            suggestions: RepoCi.requiredSuggestions(runs, jobs: jobs, required: config.requiredChecks),
            secrets: settings?.secrets,
            githubStatuses: config.githubStatuses,
            actionsRunners: config.actionsRunners,
            manager: manager
        )
        CiSurface(
            surfaceId: "repo-ci",
            tree: tree,
            data: ["required": Self.values(config.requiredChecks)],
            handle: handle
        ) { name, context, payload in
            func checked() -> Bool? {
                if case let .bool(value)? = payload?["checked"] { return value }
                return nil
            }
            switch name {
            case RepoCiView.Events.required:
                guard case let .array(raw)? = payload?["values"] else { return }
                let next = RepoCiView.requiredFrom(raw.map { value in
                    if case let .string(text) = value { return text }
                    return value.compactString
                })
                // The stored list wins (a dropped duplicate leaves the field).
                handle.setData(RepoCiView.requiredPath, Self.values(next))
                if manager, next != config.requiredChecks { onSetRequired(next) }
            case RepoCiView.Events.githubStatuses:
                if let on = checked(), manager, on != config.githubStatuses { onSetSwitch(.githubStatuses, on) }
            case RepoCiView.Events.actionsRunners:
                if let on = checked(), manager, on != config.actionsRunners { onSetSwitch(.actionsRunners, on) }
            case RepoCiView.Events.deleteSecret:
                if manager, case let .string(secret)? = context?["name"] { deleting = DeleteTarget(name: secret) }
            case RepoCiView.Events.addSecret:
                if manager { adding = true }
            default:
                break
            }
        }
        .onChange(of: config.requiredChecks) { _, required in
            handle.setData(RepoCiView.requiredPath, Self.values(required))
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(RepoCi.TestIds.ci)
        .sheet(isPresented: $adding) {
            AddSecretSheet(onSave: onSaveSecret)
        }
        // The app's own alert card, on a zero-size node of its own (EXP-240).
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $deleting) { target in
                    GlassAlert(
                        title: RepoCi.Copy.Secrets.deleteTitle,
                        message: nil,
                        actions: [
                            GlassAlertAction(RepoCi.Copy.Secrets.cancel, role: .outline, isDefault: true, isCancel: true, id: "cancel") {},
                            GlassAlertAction(RepoCi.Copy.Secrets.deleteConfirm, role: .destructive, id: "repo-ci-secret-delete-confirm") {
                                onDeleteSecret(target.name)
                            },
                        ]
                    )
                }
        }
    }
}

/// Add secret, the phones' Drawer as THE sheet: Name (validated by
/// `secretName`, invalid = inline), Value (a password field), Save as the
/// sheet's pinned action. A failure keeps the sheet open.
struct AddSecretSheet: View {
    var onSave: @MainActor (String, String) async throws -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var value = ""
    @State private var pending = false

    private var validName: Bool { RepoCi.secretName(name) }
    private var valid: Bool { validName && !value.isEmpty }

    var body: some View {
        GlassSheetChrome(title: RepoCi.Copy.Secrets.add) {
            VStack(alignment: .leading, spacing: 8) {
                VStack(spacing: 0) {
                    HStack(spacing: 12) {
                        Text(RepoCi.Copy.Secrets.name)
                            .foregroundStyle(.white)
                            .fixedSize()
                        GlassTextField("NPM_TOKEN", text: $name, bordered: false, accessibilityIdentifier: "repo-ci-secret-name")
                            .font(.expMono(.body))
                            .multilineTextAlignment(.trailing)
                            .textInputAutocapitalization(.characters)
                            .autocorrectionDisabled()
                            .onChange(of: name) { _, next in
                                let trimmed = String(next.trimmingCharacters(in: .whitespaces).prefix(128))
                                if trimmed != next { name = trimmed }
                            }
                    }
                    .formRowPadding()
                    HStack(spacing: 12) {
                        Text(RepoCi.Copy.Secrets.value)
                            .foregroundStyle(.white)
                            .fixedSize()
                        GlassTextField("", text: $value, isSecure: true, bordered: false, accessibilityIdentifier: "repo-ci-secret-value")
                            .multilineTextAlignment(.trailing)
                            .onSubmit { save() }
                    }
                    .formRowPadding()
                }
                .glassSection()
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("repo-ci-secret-form")
                if !name.isEmpty, !validName {
                    Text(RepoCi.Copy.Secrets.invalid)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .padding(.horizontal, 12)
                        .accessibilityIdentifier("repo-ci-secret-invalid")
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        } primaryAction: {
            Button { save() } label: {
                GlassSubmitLabel(RepoCi.Copy.Secrets.save, enabled: valid && !pending, loading: pending)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!valid || pending)
            .accessibilityIdentifier("repo-ci-secret-save")
        }
    }

    private func save() {
        guard valid, !pending else { return }
        pending = true
        Task {
            defer { pending = false }
            do {
                try await onSave(name, value)
                dismiss()
            } catch {}
        }
    }
}

/// The live CI half of the repo page: the repository's synced `ci_runs` +
/// `ci_jobs` (GRDB) and `ci.repoSettings`, writing through `CiApi`. A failed
/// write = a toast with Retry, then a re-read puts the stored values back.
@MainActor
@Observable
final class RepoCiModel {
    private(set) var runs: [RepoCi.Run] = []
    private(set) var jobs: [RepoCi.Job] = []
    /// nil = `ci.repoSettings` still loading.
    private(set) var settings: RepoCiSettings?

    private struct Snapshot: Equatable {
        let runs: [CiRunEntity]
        let jobs: [CiJobEntity]
    }

    func observe(pool: DatabasePool, repositoryId: String) async {
        let observation = ValueObservation.tracking { db -> Snapshot in
            let runs = try CiRunEntity.filter(Column("repository_id") == repositoryId).fetchAll(db)
            let jobs = try CiJobEntity.filter(runs.map(\.id).contains(Column("run_id"))).fetchAll(db)
            return Snapshot(runs: runs, jobs: jobs)
        }
        do {
            for try await next in observation.removeDuplicates().values(in: pool) {
                runs = next.runs.map {
                    RepoCi.Run(
                        id: $0.id, number: $0.number, name: $0.name, sha: $0.sha, ref: $0.ref, trigger: $0.trigger,
                        state: $0.state, startedAt: $0.startedAt, completedAt: $0.completedAt
                    )
                }
                jobs = next.jobs.map { RepoCi.Job(runId: $0.runId, status: $0.status, conclusion: $0.conclusion, check: $0.check) }
            }
        } catch {}
    }

    func load(api: CiApi, accountId: String, repositoryId: String, toaster: Toaster) async {
        do {
            settings = try await api.repoSettings(accountId: accountId, repositoryId: repositoryId)
        } catch {
            if settings == nil { settings = RepoCiSettings() }
            toaster.error(FeedbackStates.Errors.title(verb: "load", thing: "the CI settings"), description: error.trpcUserMessage)
        }
    }

    /// Answers at once, then the write and a re-read.
    func patch(_ change: (inout RepoCiConfig) -> Void) {
        guard var next = settings else { return }
        change(&next.config)
        settings = next
    }

    func reload(api: CiApi, accountId: String, repositoryId: String) async {
        if let fresh = try? await api.repoSettings(accountId: accountId, repositoryId: repositoryId) { settings = fresh }
    }
}

/// A run's live duration ticks while one is running.
struct RepoRunsLive: View {
    let model: RepoCiModel
    var onOpenRun: (String) -> Void

    @State private var now = Date()

    var body: some View {
        let running = model.runs.contains { $0.state == "pending" && $0.startedAt != nil }
        RepoRunsTab(rows: RepoCi.runRows(model.runs, jobs: model.jobs, now: now), onOpenRun: onOpenRun)
            .task(id: running) {
                while running, !Task.isCancelled {
                    try? await Task.sleep(for: .seconds(1))
                    now = Date()
                }
            }
    }
}

/// The CI section over the live model (managers = a team owner or the member
/// who shared the repo).
struct RepoCiLive: View {
    let model: RepoCiModel
    let repositoryId: String
    let provider: String
    let manager: Bool

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster

    var body: some View {
        RepoCiSectionView(
            provider: provider,
            runs: model.runs,
            jobs: model.jobs,
            settings: model.settings,
            manager: manager,
            onSetRequired: { checks in
                model.patch { $0.requiredChecks = checks }
                write(verb: "save", thing: "the required checks") {
                    try await deps.ciApi.setRepoConfig(accountId: accountId, repositoryId: repositoryId, requiredChecks: checks)
                }
            },
            onSetSwitch: { key, on in
                model.patch { config in
                    switch key {
                    case .githubStatuses: config.githubStatuses = on
                    case .actionsRunners: config.actionsRunners = on
                    }
                }
                write(verb: "save", thing: "the CI settings") {
                    switch key {
                    case .githubStatuses:
                        try await deps.ciApi.setRepoConfig(accountId: accountId, repositoryId: repositoryId, githubStatuses: on)
                    case .actionsRunners:
                        try await deps.ciApi.setRepoConfig(accountId: accountId, repositoryId: repositoryId, actionsRunners: on)
                    }
                }
            },
            onSaveSecret: { name, value in
                do {
                    try await deps.ciApi.setSecret(accountId: accountId, repositoryId: repositoryId, name: name, value: value)
                    await model.reload(api: deps.ciApi, accountId: accountId, repositoryId: repositoryId)
                } catch {
                    toaster.error(FeedbackStates.Errors.title(verb: "save", thing: "the secret"), description: error.trpcUserMessage)
                    throw error
                }
            },
            onDeleteSecret: { name in
                write(verb: "delete", thing: "the secret") {
                    try await deps.ciApi.deleteSecret(accountId: accountId, repositoryId: repositoryId, name: name)
                }
            }
        )
        .task(id: "\(accountId)/\(repositoryId)") {
            await model.load(api: deps.ciApi, accountId: accountId, repositoryId: repositoryId, toaster: toaster)
        }
    }

    private func write(verb: String, thing: String, _ operation: @escaping @MainActor () async throws -> Void) {
        Task {
            do {
                try await operation()
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: verb, thing: thing),
                    description: error.trpcUserMessage,
                    action: ToastAction(label: FeedbackStates.Errors.retry) {
                        Task { @MainActor in write(verb: verb, thing: thing, operation) }
                    }
                )
            }
            await model.reload(api: deps.ciApi, accountId: accountId, repositoryId: repositoryId)
        }
    }
}

private extension View {
    func formRowPadding() -> some View {
        padding(.horizontal, 12)
            .padding(.vertical, 10)
    }
}
