import ExpCore
import ExpUI
import SwiftUI

/// EXP-1291: the iOS styleguide's Settings → Agents specimen (v4-6-settings,
/// kind `settings-agents`), drawn by the REAL agent page form
/// (`AgentSettingsForm`) over fake rows: the "Triage" agent on the team's
/// build server, its claude login, one daily trigger and its instructions.
///
/// Capture only: the styleguide suite opens it with `-uiTestingStyleguide
/// settings-agents` on a signed-out relaunch (`ThreadCardsScreen`'s seam);
/// a Schedule row opens the real trigger form sheet over it, and
/// `settings-agents-trigger` opens with that sheet already up.
struct AgentSettingsStyleguideScreen: View {
    let kind: String

    static let kinds: Set<String> = ["settings-agents", "settings-agents-trigger"]

    @State private var draft = Self.triageDraft()
    @State private var editingTrigger: TriggerEdit?

    /// The sheet's item: nil `trigger` = Add.
    private struct TriggerEdit: Identifiable {
        let trigger: AgentTrigger?
        var id: String { trigger?.id ?? "new" }
    }

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()
                AgentSettingsForm(
                    draft: draft,
                    devices: [Self.buildServer],
                    canEdit: true,
                    onOpenTrigger: { editingTrigger = TriggerEdit(trigger: $0) },
                    onDelete: {}
                )
            }
            .navigationTitle(draft.name)
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        }
        .sheet(item: $editingTrigger) { edit in
            TriggerFormSheet(teamId: "team-acme", editing: edit.trigger, onSubmit: { _ in })
        }
        .onAppear {
            if kind == "settings-agents-trigger" {
                editingTrigger = TriggerEdit(trigger: draft.triggers.first)
            }
        }
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-settings-styleguide-screen")
    }

    // MARK: - Fake rows

    private static let buildServer = SteerDevice(
        deviceId: "dev-build",
        deviceLabel: "Acme build server",
        agents: ["claude"],
        acpAgents: ["claude"],
        kind: "server",
        online: true,
        agentAccounts: [
            "claude": AgentAccount(
                signedIn: true,
                email: "demo@exponential.at",
                profiles: [
                    AgentAccountProfile(
                        id: "p-demo", active: true, signedIn: true,
                        email: "demo@exponential.at", health: "ok"
                    ),
                ]
            ),
        ],
        launchDefaults: DeviceLaunchDefaults(defaultAgent: "claude")
    )

    @MainActor
    private static func triageDraft() -> AgentSettingsDraft {
        let draft = AgentSettingsDraft()
        draft.name = "Triage"
        draft.icon = "bug"
        draft.color = "#a855f7"
        draft.deviceId = buildServer.deviceId
        draft.cli = "claude"
        draft.account = "p-demo"
        draft.triggers = [
            AgentTrigger(
                id: "trg-daily",
                when: .schedule(ScheduleTrigger(interval: "daily", minuteOfDay: 9 * 60))
            ),
        ]
        draft.loadInstructions(
            "Look after the boards: merge duplicates, add labels and priorities, "
                + "start a session for anything that needs code."
        )
        return draft
    }
}
