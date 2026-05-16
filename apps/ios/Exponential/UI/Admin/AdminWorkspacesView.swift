import SwiftUI

struct AdminWorkspacesView: View {
    @Environment(AppDependencies.self) private var deps
    @State private var workspaces: [AdminWorkspace] = []
    @State private var loading = true
    @State private var deleteTarget: AdminWorkspace?

    var body: some View {
        ZStack {
            AppBackground()

            if loading {
                ProgressView().tint(.white)
            } else {
                ScrollView {
                    LazyVStack(spacing: 8) {
                        ForEach(workspaces) { workspace in
                            HStack(spacing: 12) {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(workspace.name)
                                        .font(.subheadline)
                                        .foregroundStyle(.white)
                                    Text(workspace.slug)
                                        .font(.caption)
                                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                }

                                Spacer()

                                HStack(spacing: 12) {
                                    Label("\(workspace.memberCount ?? 0)", systemImage: "person.2")
                                    Label("\(workspace.projectCount ?? 0)", systemImage: "folder")
                                }
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                                Menu {
                                    Button(role: .destructive) {
                                        deleteTarget = workspace
                                    } label: {
                                        Label("Delete workspace", systemImage: "trash")
                                    }
                                } label: {
                                    Image(systemName: "ellipsis")
                                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                        .padding(6)
                                }
                            }
                            .padding(.horizontal, 14)
                            .padding(.vertical, 10)
                            .glassRow()
                        }
                    }
                    .padding(16)
                }
            }
        }
        .navigationTitle("Workspaces (\(workspaces.count))")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .alert("Delete Workspace", isPresented: .init(
            get: { deleteTarget != nil },
            set: { if !$0 { deleteTarget = nil } }
        )) {
            Button("Delete", role: .destructive) {
                if let target = deleteTarget {
                    Task {
                        try? await deps.adminApi.deleteWorkspace(workspaceId: target.id)
                        await loadWorkspaces()
                    }
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This will permanently delete the workspace and all its data.")
        }
        .task { await loadWorkspaces() }
    }

    private func loadWorkspaces() async {
        do {
            workspaces = try await deps.adminApi.listWorkspaces()
            loading = false
        } catch {
            loading = false
        }
    }
}
