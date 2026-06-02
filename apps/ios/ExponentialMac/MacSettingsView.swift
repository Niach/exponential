import ExpCore
import ExpUI
import SwiftUI

struct MacSettingsView: View {
    @Environment(MacAppDependencies.self) private var deps

    var body: some View {
        Form {
            Section("Accounts") {
                ForEach(deps.auth.accounts) { account in
                    HStack(spacing: 10) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(account.displayName)
                            if let email = account.userEmail {
                                Text(email).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                        if account.id == deps.auth.activeAccountId {
                            Text("Active").font(.caption).foregroundStyle(.secondary)
                        } else if account.token != nil {
                            Button("Switch") { deps.auth.switchAccount(id: account.id) }
                        }
                        Button(role: .destructive) {
                            signOut(account)
                        } label: {
                            Image(systemName: "rectangle.portrait.and.arrow.right")
                        }
                        .help("Sign out")
                    }
                }
                if deps.auth.accounts.isEmpty {
                    Text("No accounts").foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
    }

    private func signOut(_ account: ServerAccount) {
        let id = account.id
        Task { await deps.syncManager.signOut(accountId: id) }
        deps.auth.removeAccount(id: id)
    }
}
