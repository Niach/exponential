import ExpUI
import ExpCore
import SwiftUI

struct AdminWorkspacesView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var workspaces: [AdminWorkspace] = []
    @State private var loading = true
    @State private var deleteTarget: AdminWorkspace?
    @State private var searchText = ""

    private var filteredWorkspaces: [AdminWorkspace] {
        guard !searchText.isEmpty else { return workspaces }
        let q = searchText.lowercased()
        return workspaces.filter { ws in
            ws.name.lowercased().contains(q) || ws.slug.lowercased().contains(q)
        }
    }

    var body: some View {
        ZStack {
            AppBackground()

            if loading {
                ProgressView().tint(.white)
            } else {
                ScrollView {
                    LazyVStack(spacing: 8) {
                        TextField("Search by name or slug…", text: $searchText)
                            .textFieldStyle(.roundedBorder)
                            .padding(.horizontal, 16)
                            .padding(.bottom, 4)

                        ForEach(filteredWorkspaces) { workspace in
                            HStack(spacing: 12) {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(workspace.name)
                                        .font(.subheadline)
                                        .foregroundStyle(.white)
                                    Text(workspace.slug)
                                        .font(.caption)
                                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                }

                                if let plan = workspace.plan, !plan.isEmpty {
                                    Text(plan.capitalized)
                                        .font(.caption2.weight(.medium))
                                        .padding(.horizontal, 6)
                                        .padding(.vertical, 2)
                                        .background(Color.white.opacity(0.1))
                                        .clipShape(Capsule())
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
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
        .navigationTitle("Workspaces (\(filteredWorkspaces.count))")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .alert("Delete Workspace", isPresented: .init(
            get: { deleteTarget != nil },
            set: { if !$0 { deleteTarget = nil } }
        )) {
            Button("Delete", role: .destructive) {
                if let target = deleteTarget {
                    Task {
                        try? await deps.adminApi.deleteWorkspace(accountId: accountId, workspaceId: target.id)
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
            workspaces = try await deps.adminApi.listWorkspaces(accountId: accountId)
            loading = false
        } catch {
            loading = false
        }
    }
}
