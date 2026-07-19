import ExpCore
import ExpUI
import SwiftUI

/// First-run wizard (shared mobile onboarding spec, EXP-8 + EXP-188): a clean
/// linear flow — Step 1 welcome (app name + one-line value prop + "Get
/// started"), Step 2 create-or-join team (signups get NO auto-created team;
/// create → owner, join → paste an invite link and exit the wizard entirely),
/// Step 3 create-first-board (name + optional repository with inline GitHub
/// connect), Step 4 done → drops into the app. `onboarding.complete` (and the
/// local `needsOnboarding` flag) is flipped on the final step so the nav gate
/// in AppNavigator stops showing this screen; the join path is flipped by
/// `teamInvites.accept` server-side (mirrored locally). The server also
/// backfills onboardingCompletedAt on session reads for users who already
/// have a board in a team (lib/auth/onboarding.ts), so a stale account
/// self-heals via reconcileWithServer before the user ever creates anything.
struct OnboardingView: View {
    @Environment(AppDependencies.self) private var deps

    @State private var page = 0
    @State private var teamId: String?
    @State private var resolvingTeam = true
    @State private var teamError: String?
    // Deliberately sticky once set: flipping needsOnboarding swaps this view out.
    @State private var finishing = false

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(spacing: 0) {
                    switch page {
                    case 0: welcomePage
                    case 1: teamPage
                    case 2: boardPage
                    default: donePage
                    }
                }
                .padding(.horizontal, 32)
                .padding(.vertical, 48)
                .frame(maxWidth: .infinity)
            }
        }
        .task { await reconcileWithServer() }
    }

    // MARK: - Step 1: Welcome

    private var welcomePage: some View {
        VStack(spacing: 0) {
            Spacer().frame(height: 64)

            Text("Exponential")
                .font(.system(size: 34, weight: .bold))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)

            Spacer().frame(height: 12)

            Text("Track issues and ship with your team.")
                .font(.body)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)

            Spacer().frame(height: 48)

            primaryButton("Get started", enabled: true) {
                page = 1
            }
        }
    }

    // MARK: - Step 2: Create or join a team

    private var teamPage: some View {
        VStack(spacing: 0) {
            Text("Set up your team")
                .font(.system(size: 24, weight: .bold))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)

            Spacer().frame(height: 8)

            Text("Create a team, or join one with an invite link from a teammate.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)

            Spacer().frame(height: 24)

            Group {
                if resolvingTeam {
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small).tint(.white.opacity(0.6))
                        Text("Checking your teams…")
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                    .padding(.vertical, 32)
                } else if let teamError {
                    VStack(spacing: 12) {
                        Text(teamError)
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .multilineTextAlignment(.center)
                        primaryButton("Try again", enabled: true) {
                            Task { await resolveTeam() }
                        }
                    }
                    .padding(24)
                    .glassCard()
                } else {
                    TeamSetupView(
                        onCreated: { team in
                            teamId = team.id
                            page = 2
                        },
                        onJoined: {
                            // teamInvites.accept stamps onboardingCompletedAt
                            // server-side; mirror it locally so the nav gate
                            // exits the wizard — joiners land in the team they
                            // just joined, no board step.
                            deps.auth.markOnboardingCompleted(
                                ISO8601DateFormatter().string(from: Date())
                            )
                        }
                    )
                }
            }
        }
        .task { await resolveTeam() }
    }

    // MARK: - Step 3: Create your first board

    private var boardPage: some View {
        VStack(spacing: 0) {
            Text("Create your first board")
                .font(.system(size: 24, weight: .bold))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)

            Spacer().frame(height: 8)

            Text("Create a board to start tracking issues — connect a GitHub repo to code on it.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)

            Spacer().frame(height: 24)

            if let teamId {
                CreateBoardForm(
                    accountId: deps.auth.activeAccountId ?? "",
                    teamId: teamId,
                    minimal: true,
                    onCreated: { _ in page = 3 }
                )
                .padding(24)
                .glassCard()
            } else {
                // Unreachable in practice — the team step always sets teamId
                // before advancing here.
                ProgressView().tint(.white.opacity(0.6)).padding(.vertical, 32)
            }
        }
    }

    // MARK: - Step 4: Done

    private var donePage: some View {
        VStack(spacing: 0) {
            Spacer().frame(height: 64)

            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 44))
                .foregroundStyle(DesignTokens.Semantic.green)

            Spacer().frame(height: 20)

            Text("You're all set")
                .font(.system(size: 28, weight: .bold))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)

            Spacer().frame(height: 12)

            Text("Your first board is ready.")
                .font(.body)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)

            Spacer().frame(height: 48)

            primaryButton(finishing ? "Opening…" : "Open Exponential", enabled: !finishing) {
                Task { await finish() }
            }
        }
    }

    private func primaryButton(_ title: String, enabled: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.body.weight(.medium))
                .foregroundStyle(.white)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
        }
        .disabled(!enabled)
        .background(enabled ? Color.white.opacity(0.15) : Color.white.opacity(0.06))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
        )
    }

    // MARK: - Actions

    /// The server backfills onboardingCompletedAt on session reads for users
    /// who already have a board in a team (the unified rule in
    /// lib/auth/onboarding.ts). Re-read the session on appear so an account
    /// whose flag was still null at login self-heals here instead of showing
    /// this screen again.
    private func reconcileWithServer() async {
        guard let accountId = deps.auth.activeAccountId,
              let user = await deps.authApi.fetchSession(accountId: accountId),
              let completedAt = user.onboardingCompletedAt
        else { return }
        deps.auth.markOnboardingCompleted(completedAt)
    }

    /// Resolve an existing default team (teams.getDefault NEVER creates —
    /// EXP-188). A user who already has a membership (e.g. re-running a
    /// half-finished wizard) skips straight to the board step; a fresh signup
    /// gets the create-or-join choice.
    private func resolveTeam() async {
        guard teamId == nil, let accountId = deps.auth.activeAccountId else {
            resolvingTeam = false
            return
        }
        resolvingTeam = true
        teamError = nil
        do {
            if let team = try await deps.teamsApi.getDefault(accountId: accountId) {
                teamId = team.id
                page = 2
            }
        } catch {
            teamError = error.trpcUserMessage
        }
        resolvingTeam = false
    }

    private func finish() async {
        guard !finishing else { return }
        finishing = true
        if let accountId = deps.auth.activeAccountId {
            try? await deps.onboardingApi.complete(accountId: accountId)
        }
        deps.auth.markOnboardingCompleted(ISO8601DateFormatter().string(from: Date()))
    }
}
