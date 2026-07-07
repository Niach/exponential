import ExpUI
import ExpCore
import SwiftUI

// Installed-repo picker (web github-repo-picker.tsx): lists the repos the user's
// GitHub App is installed on and returns the chosen one to the caller. v4: it no
// longer links a repo to a project directly — instead it feeds the create-project
// inline-connect path (`repository: { fullName }`). Handles not-configured /
// not-installed (browser install hop + foreground re-query) / installed
// (searchable list). The link/upsert happens server-side in `projects.create`.
struct GithubRepoPicker: View {
    let accountId: String
    let integrationsApi: IntegrationsApi
    /// Called with the picked repo; the sheet dismisses itself afterwards.
    var onPick: (GithubPickerRepo) -> Void

    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var scenePhase
    @State private var result: GithubReposResult?
    @State private var loading = true
    @State private var query = ""
    @State private var error: String?

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        content
                        if let error {
                            Text(error).font(.caption).foregroundStyle(.red)
                        }
                    }
                    .padding(16)
                }
            }
            .navigationTitle("Add repository")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Button("Cancel") { dismiss() } }
            }
            .task { await load() }
            .onChange(of: scenePhase) { _, phase in
                // Re-query after returning from the GitHub App install flow.
                if phase == .active { Task { await load() } }
            }
        }
    }

    @ViewBuilder private var content: some View {
        if loading && result == nil {
            HStack { Spacer(); ProgressView().tint(.white); Spacer() }.padding(.vertical, 24)
        } else if let data = result, data.configured {
            if data.installed {
                installedList(data)
            } else {
                notInstalled(data)
            }
        } else {
            Text("GitHub isn't configured for this server.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }

    @ViewBuilder private func notInstalled(_ data: GithubReposResult) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Install the Exponential GitHub App to pick a repository, then come back.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Button {
                openInstall(data)
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.left.forwardslash.chevron.right")
                    Text("Connect GitHub")
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            Button {
                Task { await load() }
            } label: {
                Text("I've connected — refresh").frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
        }
    }

    @ViewBuilder private func installedList(_ data: GithubReposResult) -> some View {
        let repos = data.repos.filter {
            query.isEmpty || $0.fullName.localizedCaseInsensitiveContains(query.trimmingCharacters(in: .whitespaces))
        }
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                TextField("Search repositories…", text: $query)
                    .textFieldStyle(.plain)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .foregroundStyle(.white)
            }
            .padding(12)
            .background(Color.white.opacity(0.05))
            .clipShape(RoundedRectangle(cornerRadius: 8))

            ForEach(repos) { repo in
                Button {
                    onPick(repo)
                    dismiss()
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "chevron.left.forwardslash.chevron.right")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        Text(repo.fullName)
                            .font(.subheadline.monospaced())
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        Spacer()
                        if repo.`private` {
                            Image(systemName: "lock.fill")
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .glassRow()
                }
                .buttonStyle(.plain)
            }

            if data.hasMore {
                Button { openInstall(data) } label: {
                    Text("Don't see your repo? Manage repositories on GitHub.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .buttonStyle(.plain)
            }
        }
    }

    // Web parity (github-repo-picker.tsx): only the server-provided GitHub App
    // install URL — the old `/account/integrations` fallback was removed in v5
    // (repo management lives in workspace settings → Repositories).
    private func openInstall(_ data: GithubReposResult) {
        if let urlString = data.installUrl, let url = URL(string: urlString) {
            Platform.open(url)
        }
    }

    private func load() async {
        await MainActor.run { loading = true }
        do {
            let r = try await integrationsApi.githubRepos(accountId: accountId)
            await MainActor.run { result = r; loading = false }
        } catch {
            await MainActor.run { self.error = error.localizedDescription; loading = false }
        }
    }
}
