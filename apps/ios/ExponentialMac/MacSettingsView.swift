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
    // Account ids with a resync in flight (per-row busy state; SyncManager
    // also serializes internally — this is the UI half of that guard).
    @State private var resyncingAccountIds: Set<String> = []

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
                        if account.token != nil {
                            // Recovery hatch for a wedged local cache: wipe every
                            // synced row + offset, then refetch all shapes.
                            Button {
                                let id = account.id
                                guard !resyncingAccountIds.contains(id) else { return }
                                resyncingAccountIds.insert(id)
                                Task { @MainActor in
                                    await deps.syncManager.resync(accountId: id)
                                    resyncingAccountIds.remove(id)
                                }
                            } label: {
                                if resyncingAccountIds.contains(account.id) {
                                    ProgressView().controlSize(.small)
                                } else {
                                    Image(systemName: "arrow.triangle.2.circlepath")
                                }
                            }
                            .disabled(resyncingAccountIds.contains(account.id))
                            .help("Resync now — wipe the local cache and refetch everything")
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

    // Pure status row (EXP-2): the expu_ key is invisible plumbing — the launcher
    // auto-mints it on the first coding session. Regenerate is the only control
    // (replaces the local key and best-effort revokes ONLY this device's old id).
    @ViewBuilder
    private var personalKeyRow: some View {
        let settings = deps.codingSettings
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Text("Personal API key")
                Spacer()
                if settings.hasPersonalKey {
                    Text("Active" + (settings.personalApiKeyStart.map { " · \($0)…" } ?? ""))
                        .font(.caption).foregroundStyle(.secondary)
                    Button {
                        regenerateKey()
                    } label: {
                        if keyBusy { ProgressView().controlSize(.small) } else { Text("Regenerate") }
                    }
                    .disabled(keyBusy || deps.auth.activeAccountId == nil)
                } else {
                    Text("Created automatically when you start a coding session")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            Text("Authenticates the coding agent as you — managed automatically, written into each worktree's .mcp.json.")
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

    /// Mint a replacement key, then best-effort revoke ONLY the locally-stored
    /// old key id — never blanket-revoke (other devices hold their own keys).
    private func regenerateKey() {
        guard let accountId = deps.auth.activeAccountId else { return }
        keyBusy = true
        keyError = nil
        let oldId = deps.codingSettings.personalApiKeyId
        Task { @MainActor in
            defer { keyBusy = false }
            do {
                let minted = try await deps.usersApi.mintPersonalApiKey(
                    accountId: accountId, name: "Device: \(MacCodingLauncher.deviceLabel)")
                deps.codingSettings.setPersonalKey(minted)
                if let oldId { try? await deps.usersApi.revokePersonalApiKey(accountId: accountId, id: oldId) }
            } catch {
                keyError = error.localizedDescription
            }
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
