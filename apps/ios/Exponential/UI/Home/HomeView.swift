import SwiftUI

struct HomeView: View {
    var syncing: Bool = false
    var onWorkspaceTap: () -> Void = {}

    @Environment(AppDependencies.self) private var deps
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
                    .padding(.bottom, 24)
                }
            }
        }
        .navigationTitle(workspaceState.activeWorkspace?.name ?? "Exponential")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                avatarMenu
            }
        }
    }

    private var avatarMenu: some View {
        Menu {
            Button {
                onWorkspaceTap()
            } label: {
                Label("Switch workspace", systemImage: "rectangle.stack")
            }

            NavigationLink(value: AppRoute.settings) {
                Label("Settings", systemImage: "gearshape")
            }

            Divider()

            Button(role: .destructive) {
                deps.auth.clearToken()
                Task { await deps.syncManager.signOut() }
            } label: {
                Label("Sign out", systemImage: "rectangle.portrait.and.arrow.right")
            }
        } label: {
            avatarCircle
                .frame(width: 32, height: 32)
                .contentShape(Circle())
        }
    }

    private var avatarCircle: some View {
        Text(userInitials)
            .font(.caption.weight(.bold))
            .foregroundStyle(.white)
            .frame(width: 28, height: 28)
            .background(Color.blue.opacity(0.6))
            .clipShape(Circle())
    }

    private var userInitials: String {
        if let name = deps.auth.userName, !name.isEmpty {
            let parts = name.split(separator: " ")
            if parts.count >= 2, let first = parts.first?.first, let last = parts.last?.first {
                return "\(first)\(last)".uppercased()
            }
            if let first = name.first {
                return String(first).uppercased()
            }
        }
        if let email = deps.auth.userEmail, let first = email.first {
            return String(first).uppercased()
        }
        return "?"
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
