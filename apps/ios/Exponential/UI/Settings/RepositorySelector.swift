import ExpUI
import ExpCore
import SwiftUI

/// Required repository picker for project creation (masterplan §6). Lists the
/// workspace's already-connected registry repos (`repositories.list`) and lets
/// the user also add a brand-new repo by name via the installed-repos picker —
/// that path connects the repo inline through `projects.create`'s
/// `repository: { fullName }`. Empty state (nothing connected, nothing added):
/// an inline "Add a repository from GitHub…" picker. Binds a `ProjectRepositoryChoice`.
struct RepositorySelector: View {
    let accountId: String
    let workspaceId: String
    @Binding var selection: ProjectRepositoryChoice?

    @Environment(AppDependencies.self) private var deps

    @State private var repos: [WorkspaceRepo] = []
    @State private var loading = true
    @State private var errorText: String?
    @State private var showAddByName = false
    /// A repo added by name in this session (not yet in the registry) — shown as
    /// a selectable row and connected inline on create.
    @State private var addedRepo: GithubPickerRepo?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Repository (required)")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            if loading {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small).tint(.white.opacity(0.5))
                    Text("Loading repositories…")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .padding(.vertical, 6)
            } else {
                ForEach(repos) { repo in
                    repoRow(
                        fullName: repo.fullName,
                        isPrivate: repo.isPrivate,
                        selected: selection == .repositoryId(repo.id)
                    ) {
                        selection = .repositoryId(repo.id)
                    }
                }

                if let added = addedRepo {
                    repoRow(
                        fullName: added.fullName,
                        isPrivate: added.`private`,
                        selected: isFullNameSelected(added.fullName)
                    ) {
                        selection = .fullName(
                            added.fullName,
                            defaultBranch: added.defaultBranch,
                            isPrivate: added.`private`
                        )
                    }
                }

                if repos.isEmpty && addedRepo == nil {
                    Text("No repositories connected yet.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.vertical, 4)
                }

                // Empty state doubles as the inline "Connect GitHub" affordance
                // (shared mobile onboarding spec): the picker sheet it opens
                // handles the not-installed case with the in-app install flow.
                Button {
                    showAddByName = true
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: repos.isEmpty && addedRepo == nil
                            ? "chevron.left.forwardslash.chevron.right"
                            : "plus")
                            .font(.caption2)
                        Text(repos.isEmpty && addedRepo == nil
                            ? "Connect GitHub…"
                            : "Add another repository…")
                            .font(.caption.weight(.medium))
                    }
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .glassButton()
                }
                .buttonStyle(.plain)
            }

            if let errorText {
                Text(errorText).font(.caption).foregroundStyle(.red.opacity(0.8))
            }
        }
        .task(id: workspaceId) { await load() }
        .sheet(isPresented: $showAddByName) {
            GithubRepoPicker(
                accountId: accountId,
                workspaceId: workspaceId,
                integrationsApi: deps.integrationsApi
            ) { repo in
                addedRepo = repo
                selection = .fullName(
                    repo.fullName,
                    defaultBranch: repo.defaultBranch,
                    isPrivate: repo.`private`
                )
            }
            .presentationBackground(.ultraThinMaterial)
        }
    }

    private func isFullNameSelected(_ fullName: String) -> Bool {
        if case let .fullName(name, _, _) = selection { return name == fullName }
        return false
    }

    @ViewBuilder
    private func repoRow(fullName: String, isPrivate: Bool, selected: Bool, tap: @escaping () -> Void) -> some View {
        Button(action: tap) {
            HStack(spacing: 10) {
                Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                    .font(.caption)
                    .foregroundStyle(selected ? DesignTokens.Semantic.blue : .white.opacity(TextOpacity.tertiary))
                Image(systemName: "chevron.left.forwardslash.chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(fullName)
                    .font(.subheadline.monospaced())
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer()
                if isPrivate {
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

    private func load() async {
        loading = true
        defer { loading = false }
        do {
            repos = try await deps.repositoriesApi.list(accountId: accountId, workspaceId: workspaceId)
            errorText = nil
        } catch {
            errorText = error.localizedDescription
        }
    }
}
