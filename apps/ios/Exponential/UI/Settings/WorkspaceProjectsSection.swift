import ExpUI
import ExpCore
import SwiftUI

struct WorkspaceProjectsSection: View {
    let projects: [ProjectEntity]
    let accountId: String
    let workspaceId: String
    let isOwner: Bool
    let projectsApi: ProjectsApi
    let repositoriesApi: RepositoriesApi
    let onDelete: (ProjectEntity) -> Void

    @State private var repoTarget: ProjectEntity?
    @State private var showCreate = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Projects")
                    .font(.headline)
                    .foregroundStyle(.white)
                Text("\(projects.count)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                Spacer()

                // New projects require repo-connect rights — owner-gated, matching
                // the server's create policy.
                if isOwner {
                    Button {
                        showCreate = true
                    } label: {
                        HStack(spacing: 4) {
                            Image(systemName: "plus")
                                .font(.caption2.weight(.semibold))
                            Text("New project")
                                .font(.caption.weight(.medium))
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .glassButton()
                    }
                    .buttonStyle(.plain)
                }
            }

            if projects.isEmpty {
                Text("No projects in this workspace yet.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
            } else {
                ForEach(projects, id: \.id) { project in
                    HStack(spacing: 10) {
                        // Color dot
                        Circle()
                            .fill(Color(hex: project.color ?? "#6366f1") ?? .gray)
                            .frame(width: 10, height: 10)

                        // Project name
                        Text(project.name)
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .lineLimit(1)

                        Spacer()

                        // Backing repo (v4: one project = one repo). Read-only
                        // chip resolving the synced repositoryId to owner/name.
                        RepoNameChip(
                            accountId: accountId,
                            workspaceId: project.workspaceId,
                            repositoryId: project.repositoryId
                        )

                        // Owner-only retarget → projects.setRepository.
                        if isOwner {
                            Button {
                                repoTarget = project
                            } label: {
                                Image(systemName: "arrow.left.arrow.right")
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            }
                            .buttonStyle(.plain)
                        }

                        // Delete button
                        Button {
                            onDelete(project)
                        } label: {
                            Image(systemName: "trash")
                                .font(.caption)
                                .foregroundStyle(.red.opacity(0.5))
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .glassRow()
                }
            }
        }
        .sheet(item: $repoTarget) { project in
            ChangeRepositorySheet(
                accountId: accountId,
                project: project,
                projectsApi: projectsApi,
                repositoriesApi: repositoriesApi
            )
            .presentationBackground(.ultraThinMaterial)
        }
        .sheet(isPresented: $showCreate) {
            CreateProjectSheet(accountId: accountId, workspaceId: workspaceId)
                .presentationBackground(.ultraThinMaterial)
        }
    }
}

/// Retarget a project's backing repo to another already-connected registry repo
/// (`projects.setRepository`). Connecting a brand-new repo stays a create-project
/// / web-side flow — this picker only offers connected repos.
private struct ChangeRepositorySheet: View {
    let accountId: String
    let project: ProjectEntity
    let projectsApi: ProjectsApi
    let repositoriesApi: RepositoriesApi

    @Environment(\.dismiss) private var dismiss
    @State private var repos: [WorkspaceRepo] = []
    @State private var loading = true
    @State private var errorText: String?
    @State private var saving = false

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()
                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        Text(project.name)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))

                        if loading {
                            HStack { Spacer(); ProgressView().tint(.white); Spacer() }
                                .padding(.vertical, 24)
                        } else if repos.isEmpty {
                            Text("No repositories connected. Connect one in the workspace settings on the web first.")
                                .font(.subheadline)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        } else {
                            ForEach(repos) { repo in
                                Button {
                                    Task { await setRepo(repo) }
                                } label: {
                                    HStack(spacing: 10) {
                                        Image(systemName: repo.id == project.repositoryId
                                            ? "checkmark.circle.fill" : "circle")
                                            .font(.caption)
                                            .foregroundStyle(repo.id == project.repositoryId
                                                ? DesignTokens.Semantic.blue
                                                : .white.opacity(TextOpacity.tertiary))
                                        Text(repo.fullName)
                                            .font(.subheadline.monospaced())
                                            .foregroundStyle(.white)
                                            .lineLimit(1)
                                            .truncationMode(.middle)
                                        Spacer()
                                        if repo.isPrivate {
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
                                .disabled(saving)
                            }
                        }

                        if let errorText {
                            Text(errorText).font(.caption).foregroundStyle(.red)
                        }
                    }
                    .padding(16)
                }
            }
            .navigationTitle("Change repository")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Button("Cancel") { dismiss() } }
            }
            .task { await load() }
        }
    }

    private func load() async {
        loading = true
        defer { loading = false }
        do {
            repos = try await repositoriesApi.list(accountId: accountId, workspaceId: project.workspaceId)
            errorText = nil
        } catch {
            errorText = error.localizedDescription
        }
    }

    private func setRepo(_ repo: WorkspaceRepo) async {
        guard repo.id != project.repositoryId else { dismiss(); return }
        saving = true
        defer { saving = false }
        do {
            try await projectsApi.setRepository(
                accountId: accountId,
                projectId: project.id,
                repositoryId: repo.id
            )
            RepositoryDirectory.invalidate(accountId: accountId, workspaceId: project.workspaceId)
            dismiss()
        } catch {
            errorText = error.localizedDescription
        }
    }
}
