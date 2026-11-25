import ExpCore
import ExpPush
import ExpUI
import GRDB
import SwiftUI

// #EXP-1815 (notification-delivery.json SETTINGS): Settings › Notifications,
// pushed off the account page (the phone's Personal settings). The groups are
// `settingsRows` over the account's teams and agents: Sessions (Personal
// Assistant, Temporary sessions), then Team agents (one per team agent, the
// team's name as detail only with more than one team). Each switch writes the
// WHOLE `sourcePrefs` after `toggleSourcePref` (optimistic; a refusal reverts
// and toasts, feedback-states.json). Existing glass rows only.
@MainActor
@Observable
final class NotificationSettingsModel {
    let accountId: String
    /// nil until `notifications.emailPrefs` landed.
    var prefs: NotificationDelivery.SourcePrefs?
    var loadFailed = false
    var teams: [NotificationDelivery.SettingsTeam] = []
    var agents: [NotificationDelivery.SettingsAgent] = []
    /// Last write wins: only the newest write may revert.
    @ObservationIgnored private var saveSeq = 0

    init(accountId: String, prefs: NotificationDelivery.SourcePrefs? = nil) {
        self.accountId = accountId
        self.prefs = prefs
    }

    var groups: [NotificationDelivery.SettingsGroup] {
        NotificationDelivery.settingsRows(teams: teams, agents: agents)
    }

    func isOn(_ key: String) -> Bool {
        NotificationDelivery.sourceAllowed(prefs ?? .init(), key)
    }

    func load(deps: AppDependencies) async {
        loadFailed = false
        do {
            let stored = try await deps.notificationsApi.emailPrefs(accountId: accountId).sourcePrefs
            prefs = NotificationDelivery.SourcePrefs(
                assistant: stored?.assistant, temporary: stored?.temporary, agents: stored?.agents
            )
        } catch {
            loadFailed = true
        }
    }

    func observe(deps: AppDependencies) async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            (
                try TeamEntity.fetchAll(db).map { NotificationDelivery.SettingsTeam(id: $0.id, name: $0.name) },
                try SessionQueries.agents(db: db, teamId: nil).map {
                    NotificationDelivery.SettingsAgent(id: $0.id, teamId: $0.teamId, name: $0.name, role: $0.role)
                }
            )
        }
        do {
            for try await (teams, agents) in observation.values(in: pool) {
                self.teams = teams
                self.agents = agents
            }
        } catch {}
    }

    func toggle(_ key: String, on: Bool, deps: AppDependencies, toaster: Toaster) {
        guard let previous = prefs else { return }
        let next = NotificationDelivery.toggleSourcePref(previous, key: key, on: on)
        prefs = next
        saveSeq += 1
        let seq = saveSeq
        let accountId = accountId
        Task {
            do {
                try await deps.notificationsApi.updateSourcePrefs(
                    accountId: accountId,
                    NotificationSourcePrefs(assistant: next.assistant, temporary: next.temporary, agents: next.agents)
                )
            } catch {
                if seq == saveSeq { prefs = previous }
                toaster.error(
                    FeedbackStates.Errors.title(verb: "update", thing: "notification settings"),
                    description: error.trpcUserMessage
                )
            }
        }
    }
}

struct NotificationSettingsPage: View {
    let accountId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.toaster) private var toaster
    @State private var model: NotificationSettingsModel

    init(accountId: String) {
        self.accountId = accountId
        _model = State(initialValue: NotificationSettingsModel(accountId: accountId))
    }

    var body: some View {
        ZStack {
            AppBackground()
            Form {
                if model.prefs != nil {
                    ForEach(model.groups) { group in
                        Section {
                            ForEach(group.rows) { row in
                                GlassToggleRow(
                                    row.label,
                                    description: row.detail,
                                    isOn: Binding(
                                        get: { model.isOn(row.key) },
                                        set: { model.toggle(row.key, on: $0, deps: deps, toaster: toaster) }
                                    )
                                )
                                .accessibilityIdentifier("source-\(row.key)")
                            }
                        } header: {
                            GlassSectionHeader(group.band)
                        }
                        .listRowBackground(glassFormRowFill)
                    }
                } else if model.loadFailed {
                    Section {
                        VStack(spacing: 12) {
                            Text(FeedbackStates.Errors.title(verb: "load", thing: "notification settings"))
                                .font(.subheadline)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            GlassPill(FeedbackStates.Errors.retry, icon: AppIcons.uiRefresh, mode: .action {
                                Task { await model.load(deps: deps) }
                            })
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                    }
                    .listRowBackground(glassFormRowFill)
                } else {
                    Section {
                        SkeletonRows(rows: 2, rowHeight: 22, dot: false, dividers: false)
                    } header: {
                        GlassSectionHeader(NotificationDelivery.Copy.sessionsBand)
                    }
                    .listRowBackground(glassFormRowFill)
                }
            }
            // EXP-603: the page's own background shows through the grouped list.
            .scrollContentBackground(.hidden)
            .listSectionSpacing(8)
            .tint(DesignTokens.Palette.primary)
        }
        .navigationTitle(SettingsNav.page("notifications")?.label ?? "Notifications")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task(id: accountId) { await model.load(deps: deps) }
        .task(id: accountId) { await model.observe(deps: deps) }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("settings-notifications-page")
    }
}
