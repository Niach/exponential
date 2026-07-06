import ExpCore
import ExpUI
import SwiftUI

/// First-run wizard (web onboarding parity, wizard.tsx): a welcome page, then a
/// create-first-project page (name → auto-derived prefix → color → REQUIRED
/// repository, connected inline). `onboarding.complete` (and the local
/// `needsOnboarding` flag) is flipped after the project is created so the nav
/// gate in AppNavigator stops showing this screen. The server also backfills
/// onboardingCompletedAt on session reads for users who already have a project
/// in a non-public workspace (lib/auth/onboarding.ts), so a stale account
/// self-heals via reconcileWithServer before the user ever creates anything.
struct OnboardingView: View {
    @Environment(AppDependencies.self) private var deps

    @State private var page = 0
    @State private var workspaceId: String?
    @State private var workspaceError: String?
    // Deliberately sticky once set: flipping needsOnboarding swaps this view out.
    @State private var finishing = false

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(spacing: 0) {
                    if page == 0 {
                        welcomePage
                    } else {
                        projectPage
                    }
                }
                .padding(.horizontal, 32)
                .padding(.vertical, 48)
                .frame(maxWidth: .infinity)
            }
        }
        .task { await reconcileWithServer() }
    }

    // MARK: - Welcome

    private var welcomePage: some View {
        VStack(spacing: 0) {
            Text("Welcome to Exponential")
                .font(.system(size: 28, weight: .bold))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)

            Spacer().frame(height: 24)

            VStack(alignment: .leading, spacing: 16) {
                HStack(spacing: 12) {
                    Image(systemName: "folder.badge.plus")
                        .font(.title2)
                        .foregroundStyle(.white)
                    Text("Create your first project")
                        .font(.headline)
                        .foregroundStyle(.white)
                }

                Text("A project is backed by a GitHub repository, so the issues you track can be coded on right away. Let's set one up.")
                    .font(.body)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            .padding(24)
            .glassCard()

            Spacer().frame(height: 24)

            primaryButton("Get started", enabled: true) {
                page = 1
            }
        }
    }

    // MARK: - Create project

    private var projectPage: some View {
        VStack(spacing: 0) {
            Text("Create your first project")
                .font(.system(size: 24, weight: .bold))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)

            Spacer().frame(height: 24)

            Group {
                if let workspaceId {
                    CreateProjectForm(
                        accountId: deps.auth.activeAccountId ?? "",
                        workspaceId: workspaceId,
                        onCreated: { _ in Task { await finish() } }
                    )
                    .padding(24)
                    .glassCard()
                } else if let workspaceError {
                    VStack(spacing: 12) {
                        Text(workspaceError)
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .multilineTextAlignment(.center)
                        primaryButton("Try again", enabled: true) {
                            Task { await prepareWorkspace() }
                        }
                    }
                    .padding(24)
                    .glassCard()
                } else {
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small).tint(.white.opacity(0.6))
                        Text("Preparing your workspace…")
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                    .padding(.vertical, 32)
                }
            }
        }
        .task { await prepareWorkspace() }
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
    /// who already have a project in a non-public workspace (the unified rule
    /// in lib/auth/onboarding.ts). Re-read the session on appear so an account
    /// whose flag was still null at login self-heals here instead of showing
    /// this screen again.
    private func reconcileWithServer() async {
        guard let accountId = deps.auth.activeAccountId,
              let user = await deps.authApi.fetchSession(accountId: accountId),
              let completedAt = user.onboardingCompletedAt
        else { return }
        deps.auth.markOnboardingCompleted(completedAt)
    }

    /// Resolve (creating if needed) the default workspace the first project
    /// lands in — invited users never reach onboarding, so this is always the
    /// user's own auto-created workspace.
    private func prepareWorkspace() async {
        guard workspaceId == nil, let accountId = deps.auth.activeAccountId else { return }
        workspaceError = nil
        do {
            workspaceId = try await deps.workspacesApi.ensureDefault(accountId: accountId).id
        } catch {
            workspaceError = error.trpcUserMessage
        }
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
