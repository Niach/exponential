import ExpUI
import ExpCore
import SwiftUI

/// The create-or-join team form (EXP-188): signups get no auto-created team
/// anymore, so the first-run wizard's team step and the zero-team empty state
/// on the Issues home both offer "Create a team" (name → `teams.create`,
/// creator becomes owner) or "Join a team" (paste an invite link/token →
/// `teamInvites.accept`). Owns the API calls and restarts the sync pipeline
/// afterwards (membership changes rotate every shape's server-derived where
/// clause — the EXP-43/46 drain-lag playbook), then hands control back
/// through the callbacks.
struct TeamSetupView: View {
    /// Called after `teams.create` succeeded and the pipeline restarted.
    let onCreated: (TeamResult) -> Void
    /// Called after `teamInvites.accept` succeeded and the pipeline
    /// restarted. The server stamps onboardingCompletedAt in the same
    /// transaction, so joiners skip the rest of the wizard.
    let onJoined: () -> Void

    @Environment(AppDependencies.self) private var deps

    @State private var teamName = ""
    @State private var inviteInput = ""
    @State private var creating = false
    @State private var joining = false
    @State private var createError: String?
    @State private var joinError: String?

    private var busy: Bool { creating || joining }

    private var canCreate: Bool {
        !teamName.trimmingCharacters(in: .whitespaces).isEmpty && !busy
    }

    private var canJoin: Bool {
        !inviteInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !busy
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Create a team
            VStack(alignment: .leading, spacing: 12) {
                Text("Create a team")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white)
                Text("Start fresh. You become the owner and can invite teammates later.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                GlassTextField("e.g. Acme Inc", text: $teamName)
                    .font(.subheadline)

                if let createError {
                    Text(createError)
                        .font(.caption)
                        .foregroundStyle(.red.opacity(0.8))
                }

                GlassSubmitButton(creating ? "Creating…" : "Create team", enabled: canCreate) {
                    Task { await createTeam() }
                }
            }
            .padding(20)
            .glassCard()

            // Join a team
            VStack(alignment: .leading, spacing: 12) {
                Text("Join a team")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white)
                Text("Ask a teammate for an invite link and paste it below.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                GlassTextField("Invite link or token", text: $inviteInput)
                    .font(.subheadline)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)

                if let joinError {
                    Text(joinError)
                        .font(.caption)
                        .foregroundStyle(.red.opacity(0.8))
                }

                GlassSubmitButton(joining ? "Joining…" : "Join team", enabled: canJoin) {
                    Task { await joinTeam() }
                }
            }
            .padding(20)
            .glassCard()
        }
    }

    // MARK: - Actions

    private func createTeam() async {
        let name = teamName.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty, !busy, let accountId = deps.auth.activeAccountId else { return }
        creating = true
        createError = nil
        do {
            let team = try await deps.teamsApi.create(accountId: accountId, name: name)
            // Membership changed: relaunch the pipeline so the new team's
            // scope syncs in seconds instead of waiting out the in-flight
            // live long-polls.
            await deps.syncManager.restartPipeline(accountId: accountId)
            // Leave `creating` set — the caller swaps this view out.
            onCreated(team)
        } catch {
            createError = error.trpcUserMessage
            creating = false
        }
    }

    private func joinTeam() async {
        guard !busy, let accountId = deps.auth.activeAccountId else { return }
        guard let token = WebLinks.extractInviteToken(inviteInput) else {
            joinError = "That doesn't look like an invite link or token."
            return
        }
        joining = true
        joinError = nil
        do {
            try await deps.teamInvitesApi.accept(accountId: accountId, token: token)
            await deps.syncManager.restartPipeline(accountId: accountId)
            // Leave `joining` set — the caller swaps this view out.
            onJoined()
        } catch {
            joinError = error.trpcUserMessage
            joining = false
        }
    }
}

// Sheet wrapper for the zero-team empty-state entry point (Issues home).
// The callbacks dismiss it; the restarted pipeline syncs the new membership.
struct TeamSetupSheet: View {
    var onDone: () -> Void = {}

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        // Two inline submits (Create / Join) — this sheet has no single
        // primary action, so the chrome's pinned slot stays empty.
        GlassSheetChrome(title: "Set up a team") {
            TeamSetupView(
                onCreated: { _ in
                    onDone()
                    dismiss()
                },
                onJoined: {
                    onDone()
                    dismiss()
                }
            )
            .padding(16)
        }
        // The styleguide lane's `sg_onboarding-create-team` anchor. It waits on
        // THIS rather than on a delay: the board switcher presents this sheet
        // from its own `onDismiss`, so there is a dismissal animation between
        // the tap and the sheet appearing. `contain` keeps the two forms'
        // fields and submits queryable inside it.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("team-setup-sheet")
    }
}
