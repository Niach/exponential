import SwiftUI

struct WorkspaceMembersSection: View {
    let accountId: String
    let members: [WorkspaceMemberEntity]
    let users: [UserEntity]
    let currentUserId: String?
    let membersApi: WorkspaceMembersApi

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Members")
                    .font(.headline)
                    .foregroundStyle(.white)
                Text("\(members.count)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }

            ForEach(members, id: \.id) { member in
                let user = users.first { $0.id == member.userId }
                HStack(spacing: 12) {
                    // Avatar
                    let initial = (user?.name ?? user?.email ?? "?").prefix(1).uppercased()
                    Text(initial)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white)
                        .frame(width: 32, height: 32)
                        .background(Color.white.opacity(0.15))
                        .clipShape(Circle())

                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 4) {
                            Text(user?.name ?? "Unknown")
                                .font(.subheadline)
                                .foregroundStyle(.white)
                            if member.userId == currentUserId {
                                Text("(you)")
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            }
                        }
                        Text(user?.email ?? "")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }

                    Spacer()

                    // Role badge
                    Text(member.role)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 8)
                        .padding(.vertical, 3)
                        .glassButton()

                    // Actions menu
                    Menu {
                        if member.role != DomainContract.workspaceRoleOwner {
                            Button {
                                Task { try? await membersApi.updateRole(accountId: accountId, memberId: member.id, role: DomainContract.workspaceRoleOwner) }
                            } label: {
                                Label("Make owner", systemImage: "crown")
                            }
                        }
                        if member.role != DomainContract.workspaceRoleMember {
                            Button {
                                Task { try? await membersApi.updateRole(accountId: accountId, memberId: member.id, role: DomainContract.workspaceRoleMember) }
                            } label: {
                                Label("Make member", systemImage: "shield")
                            }
                        }
                        Button(role: .destructive) {
                            Task { try? await membersApi.remove(accountId: accountId, memberId: member.id) }
                        } label: {
                            Label(member.userId == currentUserId ? "Leave" : "Remove", systemImage: "xmark")
                        }
                    } label: {
                        Image(systemName: "ellipsis")
                            .font(.body)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .padding(6)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .glassRow()
            }
        }
    }
}
