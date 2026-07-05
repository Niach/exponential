import ExpCore
import ExpUI
import SwiftUI

/// First-run screen. The mobile app is a companion — workspaces and projects
/// are created on the web or desktop app, so onboarding is a single informational
/// screen instead of a create-project/issue wizard. `onboarding.complete` (and the
/// local `needsOnboarding` flag) is flipped on Continue so the nav gate in
/// AppNavigator stops showing this screen. The server also backfills
/// onboardingCompletedAt on session reads for users who already have a project in a
/// non-public workspace (lib/auth/onboarding.ts), so a stale account self-heals via
/// reconcileWithServer before the user ever taps Continue.
struct OnboardingView: View {
    @Environment(AppDependencies.self) private var deps

    @State private var busy = false

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(spacing: 0) {
                    Text("Welcome to Exponential")
                        .font(.system(size: 28, weight: .bold))
                        .foregroundStyle(.white)
                        .multilineTextAlignment(.center)

                    Spacer().frame(height: 24)

                    VStack(alignment: .leading, spacing: 16) {
                        HStack(spacing: 12) {
                            Image(systemName: "laptopcomputer.and.iphone")
                                .font(.title2)
                                .foregroundStyle(.white)
                            Text("Create your first project on the web or desktop app")
                                .font(.headline)
                                .foregroundStyle(.white)
                        }

                        Text("This app is your companion for tracking and updating issues on the go. Set up workspaces and projects — and start coding — from the web or desktop app, then everything syncs here.")
                            .font(.body)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))

                        if let host = instanceHost {
                            Text(host)
                                .font(.body.monospaced())
                                .foregroundStyle(.white)
                                .padding(.horizontal, 14)
                                .padding(.vertical, 10)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(Color.white.opacity(0.06))
                                .clipShape(RoundedRectangle(cornerRadius: 10))
                        }
                    }
                    .padding(24)
                    .glassCard()

                    Spacer().frame(height: 24)

                    primaryButton(busy ? "Finishing…" : "Continue", enabled: !busy) {
                        Task { await finish() }
                    }
                }
                .padding(.horizontal, 32)
                .padding(.vertical, 48)
                .frame(maxWidth: .infinity)
            }
        }
        .task { await reconcileWithServer() }
    }

    private var instanceHost: String? {
        guard let base = deps.auth.instanceUrl,
              let url = URL(string: base) else { return nil }
        return url.host ?? base
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

    // Deliberately leaves `busy` set: flipping needsOnboarding swaps this view
    // out, and re-enabling the button first would open a double-submit window.
    private func finish() async {
        guard !busy else { return }
        busy = true
        if let accountId = deps.auth.activeAccountId {
            try? await deps.onboardingApi.complete(accountId: accountId)
        }
        deps.auth.markOnboardingCompleted(ISO8601DateFormatter().string(from: Date()))
    }
}
