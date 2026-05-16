import SwiftUI

struct HomeView: View {
    var syncing: Bool = false

    @Environment(WorkspaceState.self) private var workspaceState

    var body: some View {
        ZStack {
            AppBackground()

            if syncing && workspaceState.workspaces.isEmpty {
                VStack(spacing: 12) {
                    ProgressView()
                        .tint(.white)
                    Text("Syncing...")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            } else {
                ScrollView {
                    LazyVStack(spacing: 8) {
                        ForEach(workspaceState.filteredProjects) { project in
                            NavigationLink(value: AppRoute.project(id: project.id)) {
                                projectRow(project)
                            }
                            .buttonStyle(.plain)
                        }

                        if workspaceState.filteredProjects.isEmpty && !syncing {
                            VStack(spacing: 8) {
                                Image(systemName: "tray")
                                    .font(.title2)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                Text("No projects yet")
                                    .font(.subheadline)
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.top, 60)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .padding(.bottom, 96)
                }
            }
        }
        .navigationTitle(workspaceState.activeWorkspace?.name ?? "Exponential")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
    }

    @ViewBuilder
    private func projectRow(_ project: ProjectEntity) -> some View {
        HStack(spacing: 12) {
            Circle()
                .fill(Color(hex: project.color ?? "#888888") ?? .gray)
                .frame(width: 10, height: 10)

            Text(project.name)
                .font(.body)
                .foregroundStyle(.white)

            Spacer()

            Text(project.prefix)
                .font(.caption.monospaced())
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            Image(systemName: "chevron.right")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .glassRow()
    }
}
