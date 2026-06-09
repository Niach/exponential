import ExpUI
import ExpCore
import SwiftUI

// Cross-platform-parity GitHub repo picker (web github-repo-picker.tsx): lists
// the repos the user's GitHub App is installed on and links the chosen one to a
// project. Handles not-configured / not-installed (browser install hop +
// foreground re-query) / installed (searchable list). Hosted from workspace
// settings; the link/unlink mutations are owner-gated server-side.
struct GithubRepoPicker: View {
    let accountId: String
    let projectId: String
    let projectName: String
    let currentRepo: String?
    let integrationsApi: IntegrationsApi
    let projectsApi: ProjectsApi
    let installBaseURL: URL?

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
                        Text(projectName)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        content
                        if let error {
                            Text(error).font(.caption).foregroundStyle(.red)
                        }
                        if let current = currentRepo, !current.isEmpty {
                            Button(role: .destructive) {
                                Task { await unlink() }
                            } label: {
                                Text("Unlink \(current)").frame(maxWidth: .infinity)
                            }
                            .buttonStyle(.bordered)
                        }
                    }
                    .padding(16)
                }
            }
            .navigationTitle("Connect repo")
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
                    Task { await link(repo.fullName) }
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
                        if repo.fullName == currentRepo {
                            Text("Linked").font(.caption2).foregroundStyle(DesignTokens.Semantic.blue)
                        }
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

    private func openInstall(_ data: GithubReposResult) {
        if let urlString = data.installUrl, let url = URL(string: urlString) {
            Platform.open(url)
        } else if let base = installBaseURL {
            Platform.open(base.appending(path: "account/integrations"))
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

    private func link(_ repo: String) async {
        do {
            try await projectsApi.linkGithubRepo(accountId: accountId, projectId: projectId, repo: repo)
            await MainActor.run { dismiss() }
        } catch {
            await MainActor.run { self.error = error.localizedDescription }
        }
    }

    private func unlink() async {
        do {
            try await projectsApi.unlinkGithubRepo(accountId: accountId, projectId: projectId)
            await MainActor.run { dismiss() }
        } catch {
            await MainActor.run { self.error = error.localizedDescription }
        }
    }
}
