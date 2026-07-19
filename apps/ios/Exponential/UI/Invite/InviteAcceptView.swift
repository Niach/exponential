import ExpUI
import SwiftUI

struct InviteAcceptView: View {
    let token: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @State private var loading = true
    @State private var accepted = false
    @State private var error: String?

    var body: some View {
        ZStack {
            AppBackground()

            VStack(spacing: 20) {
                Image(systemName: accepted ? "checkmark.circle.fill" : "person.2")
                    .font(.system(size: 48))
                    .foregroundStyle(accepted ? .green : .white.opacity(TextOpacity.secondary))

                if loading {
                    ProgressView().tint(.white)
                    Text("Accepting invite...")
                        .font(.body)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                } else if accepted {
                    Text("Welcome!")
                        .font(.title2.weight(.bold))
                        .foregroundStyle(.white)
                    Text("Redirecting...")
                        .font(.body)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                } else if let error {
                    Text("Invite Failed")
                        .font(.title2.weight(.bold))
                        .foregroundStyle(.white)
                    Text(error)
                        .font(.body)
                        .foregroundStyle(.red)
                        .multilineTextAlignment(.center)
                }
            }
            .padding(32)
            .glassCard()
            .padding(32)
        }
        .task {
            do {
                try await deps.teamInvitesApi.accept(accountId: accountId, token: token)
                accepted = true
                loading = false
                // Membership just changed: every shape's server-derived where
                // clause rotated, and the in-flight live long-polls would keep
                // the OLD scope for up to ~60s. Relaunch the pipeline so the
                // joined team syncs in seconds (EXP-43 drain-lag fix).
                await deps.syncManager.restartPipeline(accountId: accountId)
                try? await Task.sleep(for: .seconds(1.5))
                dismiss()
            } catch {
                self.error = error.localizedDescription
                loading = false
            }
        }
    }
}
