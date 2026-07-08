import ExpUI
import ExpCore
import SwiftUI

/// Per-server management screen. Reached from Settings → Servers → tap a row.
/// Centralizes the actions that used to be split between the top-of-Settings
/// "Sign out" button (which only signed out the active account) and the
/// long-press context menu on the server list (which only offered "Remove").
struct ServerDetailView: View {
    let accountId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @State private var showRemoveConfirm = false
    @State private var showDeleteAccountConfirm = false
    @State private var deletingAccount = false
    @State private var deleteAccountError: String?
    @State private var resyncing = false

    private var account: ServerAccount? {
        deps.auth.accounts.first { $0.id == accountId }
    }

    /// The bundled cloud (prod or staging) — "Remove server" is nonsensical for
    /// it (it's the app's built-in instance, not a user-added server), so that
    /// action is hidden. Sign out / delete account / resync stay available.
    private var isBuiltInCloud: Bool {
        guard let base = WebLinks.normalizedBase(account?.instanceUrl) else { return false }
        return base == WebLinks.normalizedBase(AppConstants.publicCloudUrl)
            || base == WebLinks.normalizedBase(AppConstants.stagingCloudUrl)
    }

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    identitySection
                    actionsSection
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 96)
            }
        }
        .navigationTitle(account?.displayName ?? "Server")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .alert(
            "Remove \(account?.displayName ?? "server")?",
            isPresented: $showRemoveConfirm
        ) {
            Button("Cancel", role: .cancel) {}
            Button("Remove", role: .destructive) {
                guard let account else { return }
                Task { await deps.syncManager.signOut(accountId: account.id) }
                deps.auth.removeAccount(id: account.id)
                deps.db.closePool(forAccountId: account.id)
                DatabaseManager.deleteFiles(forAccountId: account.id)
                dismiss()
            }
        } message: {
            Text("This will sign you out and delete cached data for this server. The server can be re-added at any time.")
        }
        .alert(
            "Delete your account?",
            isPresented: $showDeleteAccountConfirm
        ) {
            Button("Cancel", role: .cancel) {}
            Button("Delete account", role: .destructive) {
                Task { await deleteAccount() }
            }
        } message: {
            Text("This permanently deletes your account on \(account?.displayName ?? "this server"), including your personal workspaces, issues, and comments. This cannot be undone.")
        }
        .alert(
            "Couldn't delete account",
            isPresented: Binding(
                get: { deleteAccountError != nil },
                set: { if !$0 { deleteAccountError = nil } }
            )
        ) {
            Button("OK", role: .cancel) { deleteAccountError = nil }
        } message: {
            Text(deleteAccountError ?? "")
        }
    }

    /// Server-side deletion first; only on success tear down the local
    /// account + cache (mirrors the "Remove server" path).
    private func deleteAccount() async {
        guard let account, !deletingAccount else { return }
        deletingAccount = true
        defer { deletingAccount = false }
        do {
            try await deps.usersApi.deleteAccount(accountId: account.id)
        } catch {
            deleteAccountError = error.trpcUserMessage
            return
        }
        await deps.syncManager.signOut(accountId: account.id)
        deps.auth.removeAccount(id: account.id)
        deps.db.closePool(forAccountId: account.id)
        DatabaseManager.deleteFiles(forAccountId: account.id)
        dismiss()
    }

    private var identitySection: some View {
        sectionStack(title: nil) {
            HStack(spacing: 10) {
                Image(systemName: "server.rack")
                    .font(.title3)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                VStack(alignment: .leading, spacing: 2) {
                    Text(account?.displayName ?? "")
                        .font(.body)
                        .foregroundStyle(.white)
                    if let email = account?.userEmail, !email.isEmpty {
                        Text(email)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                    Text(account?.token == nil ? "Signed out" : "Signed in")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                }
                Spacer()
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .glassRow()
        }
    }

    private var actionsSection: some View {
        sectionStack(title: nil) {
            VStack(spacing: 6) {
                if account?.token != nil {
                    // Recovery hatch for a wedged local cache: wipe every synced
                    // row + offset and refetch all shapes from scratch.
                    Button {
                        guard !resyncing else { return }
                        resyncing = true
                        Task {
                            await deps.syncManager.resync(accountId: accountId)
                            resyncing = false
                        }
                    } label: {
                        actionRow(
                            icon: "arrow.triangle.2.circlepath",
                            title: resyncing ? "Resyncing…" : "Resync now",
                            tint: .white
                        )
                    }
                    .buttonStyle(.plain)
                    .disabled(resyncing)

                    Button {
                        Task {
                            await deps.syncManager.signOut(accountId: accountId)
                            deps.auth.removeAccount(id: accountId)
                            // Re-add the URL so the user can re-auth via the
                            // login flow without losing the entry. Better-auth
                            // expects a fresh session anyway.
                            if let url = account?.instanceUrl {
                                deps.auth.setInstanceUrl(url)
                            }
                            dismiss()
                        }
                    } label: {
                        actionRow(
                            icon: "rectangle.portrait.and.arrow.right",
                            title: "Sign out",
                            tint: .red
                        )
                    }
                    .buttonStyle(.plain)

                    // App Store guideline 5.1.1(v): account deletion must be
                    // initiable in-app, not via email.
                    Button {
                        showDeleteAccountConfirm = true
                    } label: {
                        actionRow(
                            icon: "person.crop.circle.badge.xmark",
                            title: deletingAccount ? "Deleting account…" : "Delete account",
                            tint: .red
                        )
                    }
                    .buttonStyle(.plain)
                    .disabled(deletingAccount)
                } else if let url = account?.instanceUrl {
                    Button {
                        deps.auth.setInstanceUrl(url)
                        dismiss()
                    } label: {
                        actionRow(
                            icon: "arrow.clockwise",
                            title: "Reauthenticate",
                            tint: .white
                        )
                    }
                    .buttonStyle(.plain)
                }

                if !isBuiltInCloud {
                    Button {
                        showRemoveConfirm = true
                    } label: {
                        actionRow(
                            icon: "trash",
                            title: "Remove server",
                            tint: .red
                        )
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    @ViewBuilder
    private func sectionStack<Content: View>(title: String?, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            if let title {
                Text(title)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 4)
            }
            content()
        }
    }

    private func actionRow(icon: String, title: String, tint: Color) -> some View {
        HStack(spacing: 12) {
            Image(systemName: icon)
                .font(.body)
                .foregroundStyle(tint.opacity(tint == .red ? 0.85 : TextOpacity.secondary))
                .frame(width: 22)
            Text(title)
                .font(.body)
                .foregroundStyle(tint == .red ? .red.opacity(0.9) : .white)
            Spacer()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .glassRow()
    }
}
