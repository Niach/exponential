import SwiftUI

struct AdminUsersView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var users: [AdminUser] = []
    @State private var loading = true
    @State private var error: String?
    @State private var deleteTarget: AdminUser?
    @State private var searchText = ""

    private var filteredUsers: [AdminUser] {
        guard !searchText.isEmpty else { return users }
        let q = searchText.lowercased()
        return users.filter { user in
            (user.name?.lowercased().contains(q) ?? false) || user.email.lowercased().contains(q)
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
                        TextField("Search by name or email…", text: $searchText)
                            .textFieldStyle(.roundedBorder)
                            .padding(.horizontal, 16)
                            .padding(.bottom, 4)

                        ForEach(filteredUsers) { user in
                            HStack(spacing: 12) {
                                // Avatar
                                Text((user.name ?? user.email).prefix(1).uppercased())
                                    .font(.caption.weight(.medium))
                                    .foregroundStyle(.white)
                                    .frame(width: 32, height: 32)
                                    .background(Color.white.opacity(0.15))
                                    .clipShape(Circle())

                                VStack(alignment: .leading, spacing: 2) {
                                    HStack(spacing: 4) {
                                        Text(user.name ?? "No name")
                                            .font(.subheadline)
                                            .foregroundStyle(.white)
                                        if user.id == deps.auth.userId {
                                            Text("(you)")
                                                .font(.caption)
                                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                        }
                                    }
                                    Text(user.email)
                                        .font(.caption)
                                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                }

                                Spacer()

                                Toggle("", isOn: Binding(
                                    get: { user.isAdmin },
                                    set: { newValue in
                                        Task {
                                            try? await deps.adminApi.setUserAdmin(accountId: accountId, userId: user.id, isAdmin: newValue)
                                            await loadUsers()
                                        }
                                    }
                                ))
                                .labelsHidden()
                                .tint(.blue)

                                Menu {
                                    Button(role: .destructive) {
                                        deleteTarget = user
                                    } label: {
                                        Label("Delete user", systemImage: "trash")
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
        .navigationTitle("Users (\(filteredUsers.count))")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .alert("Delete User", isPresented: .init(
            get: { deleteTarget != nil },
            set: { if !$0 { deleteTarget = nil } }
        )) {
            Button("Delete", role: .destructive) {
                if let target = deleteTarget {
                    Task {
                        try? await deps.adminApi.deleteUser(accountId: accountId, userId: target.id)
                        await loadUsers()
                    }
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Delete \(deleteTarget?.email ?? "")?")
        }
        .task { await loadUsers() }
    }

    private func loadUsers() async {
        do {
            users = try await deps.adminApi.listUsers(accountId: accountId)
                .filter { !(($0.email ?? "").hasPrefix("agent-") && ($0.email ?? "").hasSuffix("@exponential.local")) }
            loading = false
        } catch {
            self.error = error.localizedDescription
            loading = false
        }
    }
}
