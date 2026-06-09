import ExpUI
import ExpCore
import SwiftUI

struct WorkspaceProjectsSection: View {
    let projects: [ProjectEntity]
    let accountId: String
    let projectsApi: ProjectsApi
    let integrationsApi: IntegrationsApi
    let installBaseURL: URL?
    let onDelete: (ProjectEntity) -> Void

    @State private var repoTarget: ProjectEntity?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Projects")
                    .font(.headline)
                    .foregroundStyle(.white)
                Text("\(projects.count)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
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

                        // Connect-repo affordance (installed-repos picker).
                        Button {
                            repoTarget = project
                        } label: {
                            HStack(spacing: 4) {
                                Image(systemName: "chevron.left.forwardslash.chevron.right")
                                    .font(.caption2)
                                Text(repoLabel(project))
                                    .font(.caption.monospaced())
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                            }
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(maxWidth: 110, alignment: .trailing)
                        }
                        .buttonStyle(.plain)

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
            GithubRepoPicker(
                accountId: accountId,
                projectId: project.id,
                projectName: project.name,
                currentRepo: project.githubRepo,
                integrationsApi: integrationsApi,
                projectsApi: projectsApi,
                installBaseURL: installBaseURL
            )
            .presentationBackground(.ultraThinMaterial)
        }
    }

    private func repoLabel(_ project: ProjectEntity) -> String {
        if let repo = project.githubRepo, !repo.isEmpty { return repo }
        return "Connect"
    }
}
