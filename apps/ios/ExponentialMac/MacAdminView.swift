import ExpCore
import ExpUI
import SwiftUI

/// Admin panel mirroring the iOS `AdminUsersView` / `AdminWorkspacesView`,
/// adapted to macOS chrome (a sheet with a segmented Users/Workspaces switch
/// instead of a `NavigationStack` push). Owner gates this behind
/// `deps.auth.isAdmin` in the shell footer menu.
struct MacAdminView: View {
    @Environment(MacAppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    let accountId: String

    private enum Tab: String, CaseIterable, Identifiable {
        case users = "Users"
        case workspaces = "Workspaces"
        var id: String { rawValue }
    }

    @State private var tab: Tab = .users

    @State private var users: [AdminUser] = []
    @State private var workspaces: [AdminWorkspace] = []
    @State private var loading = true
    @State private var error: String?
    @State private var userSearch = ""
    @State private var workspaceSearch = ""
    @State private var deleteUserTarget: AdminUser?
    @State private var deleteWorkspaceTarget: AdminWorkspace?

    private var filteredUsers: [AdminUser] {
        guard !userSearch.isEmpty else { return users }
        let q = userSearch.lowercased()
        return users.filter { ($0.name?.lowercased().contains(q) ?? false) || $0.email.lowercased().contains(q) }
    }

    private var filteredWorkspaces: [AdminWorkspace] {
        guard !workspaceSearch.isEmpty else { return workspaces }
        let q = workspaceSearch.lowercased()
        return workspaces.filter { $0.name.lowercased().contains(q) || $0.slug.lowercased().contains(q) }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Admin").font(.headline)
                Spacer()
                Button("Done") { dismiss() }
            }
            .padding()
            Divider()

            Picker("", selection: $tab) {
                ForEach(Tab.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(12)

            if loading {
                ProgressView().frame(maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(spacing: 8) {
                        switch tab {
                        case .users:
                            TextField("Search by name or email…", text: $userSearch)
                                .textFieldStyle(.roundedBorder)
                            ForEach(filteredUsers) { userRow($0) }
                        case .workspaces:
                            TextField("Search by name or slug…", text: $workspaceSearch)
                                .textFieldStyle(.roundedBorder)
                            ForEach(filteredWorkspaces) { workspaceRow($0) }
                        }
                    }
                    .padding(16)
                }
                if let error {
                    Text(error).font(.caption).foregroundStyle(.red).padding(.bottom, 8)
                }
            }
        }
        .frame(width: 640, height: 620)
        .task { await load() }
        .confirmationDialog(
            "Delete \(deleteUserTarget?.email ?? "")?",
            isPresented: Binding(get: { deleteUserTarget != nil }, set: { if !$0 { deleteUserTarget = nil } }),
            titleVisibility: .visible
        ) {
            Button("Delete user", role: .destructive) {
                if let t = deleteUserTarget {
                    Task { try? await deps.adminApi.deleteUser(accountId: accountId, userId: t.id); await load() }
                }
            }
            Button("Cancel", role: .cancel) {}
        }
        .confirmationDialog(
            "Delete workspace \(deleteWorkspaceTarget?.name ?? "")?",
            isPresented: Binding(get: { deleteWorkspaceTarget != nil }, set: { if !$0 { deleteWorkspaceTarget = nil } }),
            titleVisibility: .visible
        ) {
            Button("Delete workspace", role: .destructive) {
                if let t = deleteWorkspaceTarget {
                    Task { try? await deps.adminApi.deleteWorkspace(accountId: accountId, workspaceId: t.id); await load() }
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This will permanently delete the workspace and all its data.")
        }
    }

    private func userRow(_ user: AdminUser) -> some View {
        HStack(spacing: 12) {
            Text((user.name ?? user.email).prefix(1).uppercased())
                .font(.caption.weight(.medium))
                .foregroundStyle(.white)
                .frame(width: 32, height: 32)
                .background(Color.white.opacity(0.15))
                .clipShape(Circle())

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) {
                    Text(user.name ?? "No name").font(.subheadline)
                    if user.id == deps.auth.userId {
                        Text("(you)").font(.caption).foregroundStyle(.secondary)
                    }
                }
                Text(user.email).font(.caption).foregroundStyle(.secondary)
            }

            Spacer()

            Toggle("", isOn: Binding(
                get: { user.isAdmin },
                set: { newValue in
                    Task {
                        try? await deps.adminApi.setUserAdmin(accountId: accountId, userId: user.id, isAdmin: newValue)
                        await load()
                    }
                }
            ))
            .labelsHidden()
            .tint(Accent.indigo)

            Menu {
                Button(role: .destructive) { deleteUserTarget = user } label: {
                    Label("Delete user", systemImage: "trash")
                }
            } label: {
                Image(systemName: "ellipsis").foregroundStyle(.secondary)
            }
            .menuStyle(.borderlessButton).fixedSize()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .glassRow()
    }

    private func workspaceRow(_ workspace: AdminWorkspace) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(workspace.name).font(.subheadline)
                Text(workspace.slug).font(.caption).foregroundStyle(.secondary)
            }

            if let plan = workspace.plan, !plan.isEmpty {
                Text(plan.capitalized)
                    .font(.caption2.weight(.medium))
                    .padding(.horizontal, 6).padding(.vertical, 2)
                    .background(Color.white.opacity(0.1))
                    .clipShape(Capsule())
                    .foregroundStyle(.secondary)
            }

            Spacer()

            HStack(spacing: 12) {
                Label("\(workspace.memberCount ?? 0)", systemImage: "person.2")
                Label("\(workspace.projectCount ?? 0)", systemImage: "folder")
            }
            .font(.caption)
            .foregroundStyle(.secondary)

            Menu {
                Button(role: .destructive) { deleteWorkspaceTarget = workspace } label: {
                    Label("Delete workspace", systemImage: "trash")
                }
            } label: {
                Image(systemName: "ellipsis").foregroundStyle(.secondary)
            }
            .menuStyle(.borderlessButton).fixedSize()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .glassRow()
    }

    private func load() async {
        do {
            async let u = deps.adminApi.listUsers(accountId: accountId)
            async let w = deps.adminApi.listWorkspaces(accountId: accountId)
            users = try await u.filter {
                !($0.email.hasPrefix("agent-") && $0.email.hasSuffix("@exponential.local"))
            }
            workspaces = try await w
            loading = false
        } catch {
            self.error = error.localizedDescription
            loading = false
        }
    }
}
