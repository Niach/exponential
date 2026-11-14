import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// #EXP-1850 (apps.json `eventsTab`, web `AppEventsTab`): an app's EVENTS tab
/// = ONE grouped list of three groups, existing components only:
/// 1. THE trigger list of the app's agent (`AgentTriggersSection`, every
///    kind), editable by the app's managers (its creator, team owners);
/// 2. the WEBHOOKS group (`webhookRowCases`, moved here from App settings);
/// 3. RECENT RUNS (`apps.events {id}`): one row per run, the title
///    (`Apps.eventRunRow`) over the muted detail, trailing the relative time
///    and the status pill; a `woke` row opens the Chat tab; none = one muted
///    `copy.noRuns` row.
struct AppEventsTab: View {
    let app: AppListEntry
    let canManage: Bool
    /// A `woke` run opens the Chat tab.
    let onOpenChat: () -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.scenePhase) private var scenePhase
    /// The agent's stored triggers (the runs' titles).
    @State private var triggers: [AgentTrigger] = []
    @State private var triggersRaw: String?
    /// nil while the first read is in flight.
    @State private var runs: [Apps.EventsTab.Run]?

    /// Web's refetch period while the tab is open.
    private static let refresh: Duration = .seconds(15)

    var body: some View {
        Form {
            if let agentId = app.agentId {
                AgentTriggersSection(agentId: agentId, editable: canManage)
            }
            AppWebhooksSection(app: app)
            recentRuns
        }
        .scrollContentBackground(.hidden)
        .listSectionSpacing(8)
        .tint(DesignTokens.Palette.primary)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.eventsTab)
        .task(id: "\(accountId)/\(app.agentId ?? "")") { await observeAgent() }
        // Refetched while open, and whenever the agent's triggers change.
        .task(id: "\(accountId)/\(app.id)/\(triggersRaw ?? "")") {
            while !Task.isCancelled {
                await loadRuns()
                try? await Task.sleep(for: Self.refresh)
            }
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { Task { await loadRuns() } }
        }
    }

    // MARK: - Recent runs

    @ViewBuilder
    private var recentRuns: some View {
        Section {
            if let runs {
                if runs.isEmpty {
                    Text(Apps.EventsTab.Copy.noRuns)
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                } else {
                    ForEach(Array(runs.enumerated()), id: \.offset) { _, run in
                        runRow(run)
                    }
                }
            }
        } header: {
            GlassSectionHeader(Apps.EventsTab.Copy.recentRuns)
        }
        .listRowBackground(glassFormRowFill)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.recentRuns)
    }

    private func runRow(_ run: Apps.EventsTab.Run) -> some View {
        let row = Apps.eventRunRow(run, triggers: triggers)
        let woke = row.badge == .woke
        return Button(action: onOpenChat) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(row.title)
                        .font(.subheadline)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    if let detail = row.detail {
                        Text(detail)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(1)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                Text(RelativeTime.compact(wire: run.at))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                GlassPill(row.badge.label)
                    .fixedSize()
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!woke)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(Apps.TestIds.runRow)
    }

    // MARK: - Loads

    private func observeAgent() async {
        guard let agentId = app.agentId, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try AgentDefinitionEntity.fetchOne(db, key: agentId)?.triggers
        }
        do {
            for try await raw in observation.values(in: pool) {
                triggersRaw = raw
                triggers = AgentTrigger.parseList(raw)
            }
        } catch {}
    }

    /// A failed read keeps what is shown (the first one shows `noRuns`).
    private func loadRuns() async {
        let api = deps.appsApi
        do {
            let next = try await api.events(accountId: accountId, id: app.id)
            guard !Task.isCancelled else { return }
            runs = next
        } catch {
            if runs == nil, !Task.isCancelled { runs = [] }
        }
    }
}

/// #VAPP-238 (`webhookRowCases`; on the Events tab since #EXP-1850): one row
/// per hook of the published package, the name (mono) over the URL and the
/// status pill, then the URL copy pill and, for an owner (`canToggle`),
/// Create / Rotate secret. A minted secret shows ONCE in a row right under
/// its hook (this view's memory only, never refetched).
struct AppWebhooksSection: View {
    let app: AppListEntry

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var hooks: [AppHookWire] = []
    @State private var canToggle = false
    @State private var hookSecrets: [String: String] = [:]
    @State private var hookBusy: String?
    @State private var rotateTarget: String?
    @State private var showRotate = false

    var body: some View {
        Group {
            if !hooks.isEmpty {
                Section {
                    ForEach(Array(hooks.enumerated()), id: \.offset) { _, hook in
                        webhookRow(hook)
                        if let secret = hookSecrets[hook.name] {
                            webhookSecretRow(secret)
                        }
                    }
                } header: {
                    GlassSectionHeader(Apps.Copy.Webhooks.title)
                }
                .listRowBackground(glassFormRowFill)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier(Apps.TestIds.webhooks)
            }
        }
        // Refetched when the app or its published version changes.
        .task(id: "\(accountId)/\(app.id):\(app.publishedVersion)") { await load() }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $showRotate) {
                    GlassAlert(
                        prompt: Prompts.RotateWebhookSecret.copy(hook: rotateTarget ?? ""),
                        handlers: ["rotate": {
                            if let name = rotateTarget { rotateHook(name, verb: "rotate") }
                        }]
                    )
                }
        }
    }

    private func webhookRow(_ hook: AppHookWire) -> some View {
        let row = Apps.webhookRow(hook.row, isOwner: canToggle)
        return VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(hook.name)
                        .font(.expMono(.subheadline))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    Text(hook.url)
                        .font(.expMono(.caption))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                GlassPill(row.label)
            }
            HStack(spacing: 8) {
                GlassPill(Apps.Copy.Webhooks.copyUrl, icon: AppIcons.uiCopy, mode: .action {
                    UIPasteboard.general.string = hook.url
                    toaster.success(Apps.Copy.Webhooks.copied)
                })
                .fixedSize()
                if let action = row.action, let label = row.actionLabel {
                    GlassPill(label, mode: .action {
                        switch action {
                        case .mint:
                            rotateHook(hook.name, verb: "create")
                        case .rotate:
                            rotateTarget = hook.name
                            showRotate = true
                        }
                    }, enabled: hookBusy == nil)
                    .fixedSize()
                    .accessibilityIdentifier("\(Apps.TestIds.webhookRow)-\(hook.name)-\(action.rawValue)")
                }
                Spacer(minLength: 0)
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(Apps.TestIds.webhookRow)-\(hook.name)")
    }

    /// The shown-once secret: `copy.webhooks.secret` over the secret (mono,
    /// middle-truncated) and its copy pill.
    private func webhookSecretRow(_ secret: String) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(Apps.Copy.Webhooks.secret)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                Text(secret)
                    .font(.expMono(.subheadline))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            GlassPill(Apps.Copy.Webhooks.copySecret, icon: AppIcons.uiCopy, mode: .action {
                UIPasteboard.general.string = secret
                toaster.success(Apps.Copy.Webhooks.copied)
            })
            .fixedSize()
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.webhookSecret)
    }

    /// A failed read keeps the group hidden (nothing to act on).
    private func load() async {
        let summary = try? await deps.appsApi.tools(accountId: accountId, appId: app.id)
        guard !Task.isCancelled else { return }
        hooks = summary?.hooks ?? []
        canToggle = summary?.canToggle ?? false
    }

    /// `appAccess.rotateHook`: mints (no secret yet) or rotates a hook's
    /// secret; the answer's secret is kept here only and the row turns
    /// `ready`. A failure = the failed-action toast.
    private func rotateHook(_ name: String, verb: String) {
        guard canToggle, hookBusy == nil else { return }
        hookBusy = name
        let api = deps.appsApi
        let account = accountId
        let appId = app.id
        Task {
            defer { hookBusy = nil }
            do {
                let minted = try await api.rotateHook(accountId: account, appId: appId, name: name)
                hookSecrets[name] = minted.secret
                hooks = hooks.map { hook in
                    hook.name == name
                        ? AppHookWire(name: hook.name, url: hook.url, hasSecret: true, rotatedAt: hook.rotatedAt)
                        : hook
                }
            } catch {
                toaster.error(FeedbackStates.Errors.title(verb: verb, thing: "the secret"), description: error.trpcUserMessage)
            }
        }
    }
}
