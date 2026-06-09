import ExpCore
import ExpUI
import SwiftUI

/// macOS GitHub repo picker (mirrors iOS `GithubRepoPicker` / web
/// github-repo-picker.tsx): lists the repos the user's GitHub App installation
/// covers and links the chosen one to a project. Handles not-configured /
/// not-installed (browser install hop + manual refresh) / installed
/// (searchable list). Link/unlink are owner-gated server-side.
struct MacGithubRepoPicker: View {
    let accountId: String
    let projectId: String
    let projectName: String
    let currentRepo: String?
    let integrationsApi: IntegrationsApi
    let projectsApi: ProjectsApi
    let installBaseURL: URL?

    @Environment(\.dismiss) private var dismiss
    @State private var result: GithubReposResult?
    @State private var loading = true
    @State private var query = ""
    @State private var error: String?
    @State private var busy = false

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Connect repo — \(projectName)").font(.headline)
                Spacer()
                Button("Done") { dismiss() }
            }
            .padding()
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
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
                        .disabled(busy)
                    }
                }
                .padding(16)
            }
        }
        .frame(width: 440, height: 480)
        .task { await load() }
    }

    @ViewBuilder private var content: some View {
        if loading && result == nil {
            HStack { Spacer(); ProgressView().controlSize(.small); Spacer() }.padding(.vertical, 24)
        } else if let data = result, data.configured {
            if data.installed {
                installedList(data)
            } else {
                notInstalled(data)
            }
        } else {
            Text("GitHub isn't configured for this server.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder private func notInstalled(_ data: GithubReposResult) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Install the Exponential GitHub App to pick a repository, then come back and refresh.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
            HStack {
                Button("Connect GitHub") { openInstall(data) }
                    .buttonStyle(.borderedProminent)
                Button("I've connected — refresh") { Task { await load() } }
            }
        }
    }

    @ViewBuilder private func installedList(_ data: GithubReposResult) -> some View {
        let repos = data.repos.filter {
            query.isEmpty || $0.fullName.localizedCaseInsensitiveContains(query.trimmingCharacters(in: .whitespaces))
        }
        VStack(alignment: .leading, spacing: 8) {
            TextField("Search repositories…", text: $query)
                .textFieldStyle(.roundedBorder)

            ForEach(repos) { repo in
                Button {
                    Task { await link(repo.fullName) }
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "chevron.left.forwardslash.chevron.right")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text(repo.fullName)
                            .font(.subheadline.monospaced())
                            .lineLimit(1)
                        Spacer()
                        if repo.fullName == currentRepo {
                            Text("Linked").font(.caption2).foregroundStyle(.blue)
                        }
                        if repo.`private` {
                            Image(systemName: "lock.fill")
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                        }
                    }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                    .background(Color.white.opacity(0.04))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(busy)
            }

            if data.hasMore {
                Button { openInstall(data) } label: {
                    Text("Don't see your repo? Manage repositories on GitHub.")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
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
        loading = true
        do {
            result = try await integrationsApi.githubRepos(accountId: accountId)
            loading = false
        } catch {
            self.error = error.localizedDescription
            loading = false
        }
    }

    private func link(_ repo: String) async {
        busy = true
        defer { busy = false }
        do {
            try await projectsApi.linkGithubRepo(accountId: accountId, projectId: projectId, repo: repo)
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func unlink() async {
        busy = true
        defer { busy = false }
        do {
            try await projectsApi.unlinkGithubRepo(accountId: accountId, projectId: projectId)
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
    }
}
