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
/// The team step's words (polish round ×4: web `wizard.tsx` is the source).
enum TeamSetupCopy {
    static let welcomeTitle = "Welcome to Exponential"
    static let createTeam = "Create a team"
    static let joinTeam = "Join a team"
    static let teamNameLabel = "Team name"
    static let createSubmit = "Create team"
    static let inviteLinkLabel = "Invite link"
    static let invitePlaceholder = "Paste an invite link"
    static let joinSubmit = "Continue"
    static let back = "Back"
}

struct TeamSetupView: View {
    /// Called after `teams.create` succeeded and the pipeline restarted.
    let onCreated: (TeamResult) -> Void
    /// Called after `teamInvites.accept` succeeded and the pipeline
    /// restarted. The server stamps onboardingCompletedAt in the same
    /// transaction, so joiners skip the rest of the wizard (bar the join
    /// step, when they own no machine: EXP-1169).
    let onJoined: () -> Void

    /// The wizard draws web's choice page heading (the mark over "Welcome
    /// to Exponential"); the Set up a team sheet has its own chrome title.
    var showsBrandHeading = false

    @Environment(AppDependencies.self) private var deps

    /// Polish round ×4 (web `wizard.tsx` ChoiceStep): a choice page with two
    /// outline buttons that PUSH the create and join pages.
    enum Mode: Equatable {
        case choice
        case create
        case join
    }

    @State private var mode: Mode = .choice
    @State private var teamName = ""
    @State private var inviteInput = ""
    @State private var creating = false
    @State private var joining = false
    @State private var createError: String?
    @State private var joinError: String?
    @Environment(\.motion) private var motion

    private var busy: Bool { creating || joining }

    private var canCreate: Bool {
        !teamName.trimmingCharacters(in: .whitespaces).isEmpty && !busy
    }

    private var canJoin: Bool {
        !inviteInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !busy
    }

    var body: some View {
        Group {
            switch mode {
            case .choice: choicePage
            case .create: createPage
            case .join: joinPage
            }
        }
        .animation(motion.standard, value: mode)
    }

    // MARK: - Pages

    /// No card and no line explaining what a team is: the mark, the title,
    /// two outline buttons.
    private var choicePage: some View {
        VStack(spacing: 24) {
            if showsBrandHeading {
                AuthBrandHeading(title: TeamSetupCopy.welcomeTitle)
            }
            VStack(spacing: 10) {
                GlassOAuthButton(TeamSetupCopy.createTeam, action: { mode = .create }) {
                    AppIcon(AppIcons.uiAdd, size: AppIcon.Size.medium)
                }
                .accessibilityIdentifier("team-setup-create")
                GlassOAuthButton(TeamSetupCopy.joinTeam, action: { mode = .join }) {
                    AppIcon(AppIcons.uiLink, size: AppIcon.Size.medium)
                }
                .accessibilityIdentifier("team-setup-join")
            }
        }
    }

    private var createPage: some View {
        VStack(alignment: .leading, spacing: 12) {
            pageTitle(TeamSetupCopy.createTeam)

            fieldLabel(TeamSetupCopy.teamNameLabel)
            GlassTextField("e.g. Acme Inc", text: $teamName)
                .font(.subheadline)

            if let createError {
                Text(createError)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
            }

            pageFooter(
                primary: creating ? "Creating…" : TeamSetupCopy.createSubmit,
                enabled: canCreate,
                loading: creating
            ) {
                Task { await createTeam() }
            }
        }
    }

    private var joinPage: some View {
        VStack(alignment: .leading, spacing: 12) {
            pageTitle(TeamSetupCopy.joinTeam)

            fieldLabel(TeamSetupCopy.inviteLinkLabel)
            GlassTextField(TeamSetupCopy.invitePlaceholder, text: $inviteInput)
                .font(.subheadline)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)

            if let joinError {
                Text(joinError)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
            }

            pageFooter(
                primary: joining ? "Joining…" : TeamSetupCopy.joinSubmit,
                enabled: canJoin,
                loading: joining
            ) {
                Task { await joinTeam() }
            }
        }
    }

    private func pageTitle(_ title: String) -> some View {
        Text(title)
            .font(.system(size: 24, weight: .bold))
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity, alignment: .center)
            .padding(.bottom, 12)
    }

    private func fieldLabel(_ label: String) -> some View {
        Text(label)
            .font(.subheadline.weight(.medium))
            .foregroundStyle(.white)
    }

    /// Back (ghost) leading, the page's primary trailing — web's step footer.
    private func pageFooter(
        primary: String,
        enabled: Bool,
        loading: Bool,
        action: @escaping () -> Void
    ) -> some View {
        HStack(spacing: 12) {
            Button {
                createError = nil
                joinError = nil
                mode = .choice
            } label: {
                HStack(spacing: 6) {
                    AppIcon(AppIcons.uiChevronLeft, size: AppIcon.Size.small)
                    Text(TeamSetupCopy.back)
                }
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .padding(.vertical, 12)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(busy)

            GlassSubmitButton(primary, enabled: enabled, loading: loading, action: action)
        }
        .padding(.top, 8)
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

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    /// EXP-1169: joined, and the caller owns no machine, so the sheet's
    /// content swaps to the join step until its advance.
    @State private var showDevicesStep = false

    var body: some View {
        // Two inline submits (Create / Join) — this sheet has no single
        // primary action, so the chrome's pinned slot stays empty.
        GlassSheetChrome(title: "Set up a team") {
            Group {
                if showDevicesStep {
                    DevicesStepView(accountId: deps.auth.activeAccountId ?? "") {
                        onDone()
                        dismiss()
                    }
                } else {
                    TeamSetupView(
                        onCreated: { _ in
                            onDone()
                            dismiss()
                        },
                        onJoined: {
                            Task { await enterJoinedTeam() }
                        }
                    )
                }
            }
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

    private func enterJoinedTeam() async {
        var owns = await DeviceQueries.ownsDevice(
            db: deps.db,
            accountId: deps.auth.activeAccountId ?? "",
            userId: deps.auth.userId
        )
        // A fresh install may not have its devices rows yet: give the
        // shape up to 2s before offering the join step (web waits 4s).
        var waited = 0
        while !owns && waited < 8 {
            try? await Task.sleep(for: .milliseconds(250))
            waited += 1
            owns = await DeviceQueries.ownsDevice(
                db: deps.db,
                accountId: deps.auth.activeAccountId ?? "",
                userId: deps.auth.userId
            )
        }
        if owns {
            onDone()
            dismiss()
        } else {
            showDevicesStep = true
        }
    }
}
