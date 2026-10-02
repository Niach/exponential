import ExpUI
import ExpCore
import SwiftUI

struct InviteAcceptView: View {
    let token: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @State private var loading = true
    @State private var accepted = false
    @State private var error: String?
    /// EXP-1169: accepted, and the caller owns no machine, so the join step
    /// replaces the welcome card until its advance.
    @State private var showDevicesStep = false

    var body: some View {
        ZStack {
            AppBackground()

            if showDevicesStep {
                ScrollView {
                    DevicesStepView(accountId: accountId) { dismiss() }
                        .padding(.horizontal, 32)
                        .padding(.vertical, 48)
                        .frame(maxWidth: .infinity)
                }
            } else {
                statusCard
            }
        }
        .task {
            do {
                try await deps.teamInvitesApi.accept(accountId: accountId, token: token)
                // Own rows are local and survive the pipeline restart below,
                // so the read never waits on the network.
                let owns = await DeviceQueries.ownsDevice(
                    db: deps.db, accountId: accountId, userId: deps.auth.userId
                )
                accepted = owns
                loading = !owns
                // Membership just changed: every shape's server-derived where
                // clause rotated, and the in-flight live long-polls would keep
                // the OLD scope for up to ~60s. Relaunch the pipeline so the
                // joined team syncs in seconds (EXP-43 drain-lag fix).
                await deps.syncManager.restartPipeline(accountId: accountId)
                if owns {
                    try? await Task.sleep(for: .seconds(1.5))
                    dismiss()
                } else {
                    loading = false
                    showDevicesStep = true
                }
            } catch {
                self.error = error.trpcUserMessage
                loading = false
            }
        }
    }

    private var statusCard: some View {
        VStack(spacing: 20) {
            AppIcon(accepted ? AppIcons.uiSuccess : AppIcons.settingsMembers, size: 48)
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
}
