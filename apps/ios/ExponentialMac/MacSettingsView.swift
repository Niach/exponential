import ExpCore
import ExpUI
import SwiftUI

struct MacSettingsView: View {
    @Environment(MacAppDependencies.self) private var deps

    // Coding section (§4b) local UI state.
    @State private var doctorOutput: String?
    @State private var doctorBusy = false
    @State private var keyBusy = false
    @State private var keyError: String?
    @State private var copied = false

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

            codingSection
        }
        .formStyle(.grouped)
    }

    // MARK: - Coding (§4b desktop settings)

    @ViewBuilder
    private var codingSection: some View {
        Section("Coding") {
            // Claude CLI path (default resolves `claude` on PATH).
            LabeledContent("Claude CLI") {
                TextField("claude", text: settingBinding(\.claudePath))
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 260)
            }
            // Repos + worktree root.
            LabeledContent("Repos root") {
                TextField(MacCodingSettings.defaultReposRoot, text: settingBinding(\.reposRoot))
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 260)
            }
            // Branch prefix (launcher builds `<prefix><ISSUE-IDENTIFIER>`).
            LabeledContent("Branch prefix") {
                TextField("exp/", text: settingBinding(\.branchPrefix))
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 140)
            }

            // Doctor: one-shot claude/git version check.
            HStack {
                Button {
                    runDoctor()
                } label: {
                    if doctorBusy { ProgressView().controlSize(.small) } else { Text("Check tools") }
                }
                .disabled(doctorBusy)
                if let doctorOutput {
                    Text(doctorOutput)
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .lineLimit(2)
                }
            }

            personalKeyRow
        }
    }

    @ViewBuilder
    private var personalKeyRow: some View {
        let settings = deps.codingSettings
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Text("Personal API key")
                Spacer()
                if settings.hasPersonalKey {
                    Text("Set" + (settings.personalApiKeyStart.map { " · \($0)…" } ?? ""))
                        .font(.caption).foregroundStyle(.secondary)
                } else {
                    Text("Not set").font(.caption).foregroundStyle(.secondary)
                }
            }
            HStack(spacing: 8) {
                Button {
                    generateKey()
                } label: {
                    if keyBusy {
                        ProgressView().controlSize(.small)
                    } else {
                        Text(settings.hasPersonalKey ? "Regenerate" : "Generate")
                    }
                }
                .disabled(keyBusy || deps.auth.activeAccountId == nil)
                if settings.hasPersonalKey {
                    Button(copied ? "Copied" : "Copy") {
                        if let key = settings.personalApiKey {
                            Platform.copyToPasteboard(key)
                            copied = true
                            Task { @MainActor in try? await Task.sleep(for: .seconds(2)); copied = false }
                        }
                    }
                    Button("Revoke", role: .destructive) { revokeKey() }
                        .disabled(keyBusy)
                }
            }
            Text("Written into each worktree's .mcp.json so the coding agent authenticates as you.")
                .font(.caption).foregroundStyle(.secondary)
            if let keyError {
                Text(keyError).font(.caption).foregroundStyle(.red)
            }
        }
    }

    // MARK: - Actions

    /// Two-way binding to a `MacCodingSettings` string field that persists on set.
    private func settingBinding(_ keyPath: ReferenceWritableKeyPath<MacCodingSettings, String>) -> Binding<String> {
        Binding(
            get: { deps.codingSettings[keyPath: keyPath] },
            set: { newValue in
                deps.codingSettings[keyPath: keyPath] = newValue
                deps.codingSettings.save()
            }
        )
    }

    private func runDoctor() {
        doctorBusy = true
        doctorOutput = nil
        let claudePath = deps.codingSettings.claudePath
        Task { @MainActor in
            let claude = await Task.detached { PreviewShell.run(claudePath, ["--version"]) }.value
            let git = await Task.detached { PreviewShell.run("git", ["--version"]) }.value
            let claudeLine = claude.ok
                ? claude.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
                : "claude: not found"
            let gitLine = git.ok
                ? git.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
                : "git: not found"
            doctorOutput = "\(claudeLine)  ·  \(gitLine)"
            doctorBusy = false
        }
    }

    private func generateKey() {
        guard let accountId = deps.auth.activeAccountId else { return }
        keyBusy = true
        keyError = nil
        let oldId = deps.codingSettings.personalApiKeyId
        Task { @MainActor in
            defer { keyBusy = false }
            do {
                let minted = try await deps.usersApi.mintPersonalApiKey(accountId: accountId, name: "Exponential Desktop")
                deps.codingSettings.setPersonalKey(minted)
                // Best-effort revoke of the key we just replaced.
                if let oldId { try? await deps.usersApi.revokePersonalApiKey(accountId: accountId, id: oldId) }
            } catch {
                keyError = error.localizedDescription
            }
        }
    }

    private func revokeKey() {
        guard let accountId = deps.auth.activeAccountId,
              let id = deps.codingSettings.personalApiKeyId else {
            deps.codingSettings.clearPersonalKey()
            return
        }
        keyBusy = true
        keyError = nil
        Task { @MainActor in
            defer { keyBusy = false }
            do {
                try await deps.usersApi.revokePersonalApiKey(accountId: accountId, id: id)
            } catch {
                keyError = "Revoked locally; server revoke failed: \(error.localizedDescription)"
            }
            deps.codingSettings.clearPersonalKey()
        }
    }

    private func signOut(_ account: ServerAccount) {
        let id = account.id
        // Tear sync down first (it still references the token + DB pool), then
        // remove the account so we never yank state out from under the sync task.
        Task {
            await deps.syncManager.signOut(accountId: id)
            // Mac sign-out is a full account removal (unlike iOS, where the shared
            // SyncManager.signOut intentionally keeps the cache for offline
            // resume). Close the pool so we don't leak its file handles + reader
            // threads and don't reuse a stale cached pool if the same instance is
            // re-added (account ids are derived deterministically from the URL).
            deps.db.closePool(forAccountId: id)
            deps.auth.removeAccount(id: id)
        }
    }
}
