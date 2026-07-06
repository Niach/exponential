import ExpUI
import ExpCore
import SwiftUI

struct WorkspaceMembersSection: View {
    let accountId: String
    let members: [WorkspaceMemberEntity]
    let users: [UserEntity]
    let currentUserId: String?
    let membersApi: WorkspaceMembersApi
    var workspaceId: String? = nil
    var invites: [WorkspaceInviteEntity] = []
    var invitesApi: WorkspaceInvitesApi? = nil

    @State private var generatedLink: String?
    @State private var copied = false
    @State private var generating = false

    // A workspace must always keep at least one owner.
    private var ownerCount: Int {
        members.filter { $0.role == DomainContract.workspaceRoleOwner }.count
    }

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
                    let initial = memberDisplayName(user, id: member.userId).prefix(1).uppercased()
                    Text(initial)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white)
                        .frame(width: 32, height: 32)
                        .background(Color.white.opacity(0.15))
                        .clipShape(Circle())

                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 4) {
                            Text(memberDisplayName(user, id: member.userId))
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
                    let isSelf = member.userId == currentUserId
                    let isLastOwner = member.role == DomainContract.workspaceRoleOwner && ownerCount <= 1
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
                            .disabled(isLastOwner)
                        }
                        if isSelf {
                            if !isLastOwner {
                                Button(role: .destructive) {
                                    Task { try? await membersApi.remove(accountId: accountId, memberId: member.id) }
                                } label: {
                                    Label("Leave", systemImage: "xmark")
                                }
                            }
                        } else {
                            Button(role: .destructive) {
                                Task { try? await membersApi.remove(accountId: accountId, memberId: member.id) }
                            } label: {
                                Label("Remove", systemImage: "xmark")
                            }
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

            // Invite section (shown when workspaceId and invitesApi are provided)
            if let wId = workspaceId, let api = invitesApi {
                Divider()
                    .background(Color.white.opacity(0.15))
                    .padding(.vertical, 4)

                Text("Invite Members")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)

                Text("Generate a link to invite someone to this workspace.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                // Generate link button
                Button {
                    Task { await generateLink(workspaceId: wId, api: api) }
                } label: {
                    HStack(spacing: 6) {
                        if generating {
                            ProgressView().tint(.white)
                        } else {
                            Image(systemName: "link")
                        }
                        Text("Generate invite link")
                    }
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 12)
                }
                .glassButton()
                .disabled(generating)
                .buttonStyle(.plain)

                // Generated link
                if let link = generatedLink {
                    HStack {
                        Text(link)
                            .font(.caption.monospaced())
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(1)
                            .truncationMode(.middle)

                        Button {
                            UIPasteboard.general.string = link
                            copied = true
                            Task {
                                try? await Task.sleep(for: .seconds(2))
                                copied = false
                            }
                        } label: {
                            Image(systemName: copied ? "checkmark" : "doc.on.doc")
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .glassRow()
                }

                // Pending invites
                if !invites.isEmpty {
                    Text("Pending")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.top, 8)

                    ForEach(invites, id: \.id) { invite in
                        HStack {
                            Text(invite.role)
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .padding(.horizontal, 8)
                                .padding(.vertical, 3)
                                .glassButton()

                            Text("Expires \(invite.expiresAt.prefix(10))")
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                            Spacer()

                            Button {
                                Task { try? await api.revoke(accountId: accountId, inviteId: invite.id) }
                            } label: {
                                Image(systemName: "trash")
                                    .font(.caption)
                                    .foregroundStyle(.red.opacity(0.7))
                            }
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .glassRow()
                    }
                }
            }
        }
    }

    private func generateLink(workspaceId: String, api: WorkspaceInvitesApi) async {
        generating = true
        do {
            let result = try await api.create(accountId: accountId, workspaceId: workspaceId, role: DomainContract.workspaceRoleMember)
            generatedLink = "exponential://invite/\(result.token)"
        } catch {}
        generating = false
    }
}
