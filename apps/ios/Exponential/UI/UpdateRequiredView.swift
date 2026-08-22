import ExpCore
import ExpUI
import SwiftUI

/// Full-screen blocking gate (EXP-104) shown when the ACTIVE account's server
/// has rejected this client version (HTTP 426). Mirrors the glass styling of
/// LoginView / InstanceView. Scoped to one account (REV2-43): other signed-in
/// servers keep syncing behind it, and "Remove this server" is the escape for an
/// instance whose minimum can't be satisfied by any shipping build — without it
/// a single misconfigured server strands the whole app forever.
struct UpdateRequiredView: View {
    let accountId: String

    @Environment(AppDependencies.self) private var deps
    @State private var showRemoveConfirm = false

    private var account: ServerAccount? {
        deps.auth.accounts.first { $0.id == accountId }
    }

    var body: some View {
        ZStack {
            AppBackground()

            VStack(alignment: .leading, spacing: 0) {
                AppIcon(AppIcons.uiUpdate, size: AppIcon.Size.xlarge, weight: .semibold)
                    .foregroundStyle(.white)

                Spacer().frame(height: 20)

                Text("Update required")
                    .font(.system(size: 28, weight: .bold))
                    .foregroundStyle(.white)

                Spacer().frame(height: 8)

                Text(bodyText)
                    .font(.body)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                Spacer().frame(height: 24)

                actionSection

                Spacer().frame(height: 12)

                removeServerButton
            }
            .padding(24)
            .glassCard()
            .padding(.horizontal, 32)
        }
        .alert(
            "Remove \(account?.displayName ?? "server")?",
            isPresented: $showRemoveConfirm
        ) {
            Button("Cancel", role: .cancel) {}
            Button("Remove", role: .destructive) { removeServer() }
        } message: {
            Text("This will sign you out and delete cached data for this server. The server can be re-added at any time.")
        }
    }

    private var bodyText: String {
        // Name the server: on a multi-account device the gate belongs to ONE
        // instance, and the user needs to know which one is asking.
        let server = account?.displayName ?? "This server"
        let base = "\(server) no longer supports this version of Exponential. Update to the latest version to keep going."
        if let min = UpdateGate.shared.upgrade(forAccountId: accountId)?.min {
            return "\(base) The minimum supported version is \(min)."
        }
        return base
    }

    @ViewBuilder
    private var actionSection: some View {
        if AppConstants.isStaging {
            // Staging ships via TestFlight, which has no direct in-app update
            // deep link — point the user there in words.
            Text("Update the app via TestFlight")
                .font(.body.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
                .background(Color.white.opacity(0.08))
                .clipShape(RoundedRectangle(cornerRadius: 10))
                .overlay(
                    RoundedRectangle(cornerRadius: 10)
                        .stroke(Color.white.opacity(0.15), lineWidth: 0.5)
                )
        } else {
            Link(destination: AppConstants.appStoreUrl) {
                GlassSubmitLabel("Update on the App Store")
            }
        }
    }

    /// The escape hatch: a server whose minimum exceeds every shipping build
    /// can't be satisfied by updating, so removing it must always be reachable
    /// from here — Settings sits behind this gate when the account is active.
    private var removeServerButton: some View {
        Button(role: .destructive) {
            showRemoveConfirm = true
        } label: {
            Text("Remove this server")
                .font(.body.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
        }
        .buttonStyle(.plain)
        .background(Color.white.opacity(0.08))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.white.opacity(0.15), lineWidth: 0.5)
        )
    }

    /// Same teardown ServerDetailView's "Remove server" runs, plus clearing this
    /// account's gate entry so re-adding the server later starts clean. The
    /// server-side calls are best-effort: the instance is 426ing every request,
    /// so they are expected to fail and must not block the local removal.
    @MainActor
    private func removeServer() {
        guard let account else { return }
        Task {
            await deps.pushTokenManager.unregister(accountId: account.id)
            if let token = account.token {
                await deps.authApi.signOut(instanceUrl: account.instanceUrl, token: token)
            }
            await deps.syncManager.signOut(accountId: account.id)
            deps.auth.removeAccount(id: account.id)
            deps.db.closePool(forAccountId: account.id)
            DatabaseManager.deleteFiles(forAccountId: account.id)
            SharedBoardMirror.remove(accountId: account.id)
            UpdateGate.shared.clear(accountId: account.id)
        }
    }
}
