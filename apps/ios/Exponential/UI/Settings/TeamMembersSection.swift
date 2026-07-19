import ExpUI
import ExpCore
import SwiftUI

struct TeamMembersSection: View {
    let accountId: String
    let members: [TeamMemberEntity]
    let users: [UserEntity]
    let currentUserId: String?
    let membersApi: TeamMembersApi
    var teamId: String? = nil
    var invites: [TeamInviteEntity] = []
    var invitesApi: TeamInvitesApi? = nil
    // Owner-only controls (role change / remove / invite management) are HIDDEN
    // for non-owners — full web parity, not greyed. Self-leave stays for anyone.
    var isOwner: Bool = false
    // Base URL for building https invite links.
    var instanceBaseURL: URL? = nil

    @State private var generatedLink: String?
    @State private var copied = false
    @State private var generating = false
    @State private var confirm: MemberConfirm?
    @State private var inviteError: String?
    @State private var actionError: String?

    // Destructive/role actions are confirmed through a single alert.
    private enum MemberConfirm {
        case remove(TeamMemberEntity, isSelf: Bool)
        case changeRole(TeamMemberEntity, to: String)
        case revokeInvite(TeamInviteEntity)
    }

    // A team must always keep at least one owner.
    private var ownerCount: Int {
        members.filter { $0.role == DomainContract.teamRoleOwner }.count
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
                memberRow(member)
            }

            if let actionError {
                Text(actionError)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
            }

            // Invite management is owner-only (server-gated) — hidden entirely
            // for non-owners.
            if isOwner, let wId = teamId, let api = invitesApi {
                inviteSection(teamId: wId, api: api)
            }
        }
        .alert(confirmTitle, isPresented: Binding(
            get: { confirm != nil },
            set: { if !$0 { confirm = nil } }
        ), presenting: confirm) { target in
            Button("Cancel", role: .cancel) { confirm = nil }
            Button(confirmButtonLabel(target), role: isDestructive(target) ? .destructive : nil) {
                Task { await perform(target) }
            }
        } message: { target in
            Text(confirmMessage(target))
        }
    }

    // MARK: - Member row

    @ViewBuilder
    private func memberRow(_ member: TeamMemberEntity) -> some View {
        let user = users.first { $0.id == member.userId }
        let isSelf = member.userId == currentUserId
        let isLastOwner = member.role == DomainContract.teamRoleOwner && ownerCount <= 1
        HStack(spacing: 12) {
            // Avatar (inert — there is no member-profile screen)
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
                    if isSelf {
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

            // Actions menu — only rendered when there is an action to offer:
            // owners get role/remove; any member gets self-leave. A non-owner,
            // non-self row has no actions, so the ellipsis is hidden entirely.
            if isOwner || (isSelf && !isLastOwner) {
                Menu {
                    if isOwner {
                        if member.role != DomainContract.teamRoleOwner {
                            Button {
                                confirm = .changeRole(member, to: DomainContract.teamRoleOwner)
                            } label: {
                                Label("Make owner", systemImage: "crown")
                            }
                        }
                        if member.role != DomainContract.teamRoleMember {
                            Button {
                                confirm = .changeRole(member, to: DomainContract.teamRoleMember)
                            } label: {
                                Label("Make member", systemImage: "shield")
                            }
                            .disabled(isLastOwner)
                        }
                    }
                    if isSelf {
                        if !isLastOwner {
                            Button(role: .destructive) {
                                confirm = .remove(member, isSelf: true)
                            } label: {
                                Label("Leave", systemImage: "xmark")
                            }
                        }
                    } else if isOwner {
                        Button(role: .destructive) {
                            confirm = .remove(member, isSelf: false)
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
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    // MARK: - Invite section (owner-only)

    @ViewBuilder
    private func inviteSection(teamId wId: String, api: TeamInvitesApi) -> some View {
        Divider()
            .background(Color.white.opacity(0.15))
            .padding(.vertical, 4)

        Text("Invite Members")
            .font(.subheadline.weight(.medium))
            .foregroundStyle(.white)

        Text("Generate a link to invite someone to this team.")
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))

        // Generate link button
        Button {
            Task { await generateLink(teamId: wId, api: api) }
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
            // Full-capsule hit target — .plain hit-tests only opaque pixels.
            .contentShape(Rectangle())
        }
        .glassButton()
        .disabled(generating)
        .buttonStyle(.plain)

        if let inviteError {
            Text(inviteError)
                .font(.caption)
                .foregroundStyle(.red.opacity(0.8))
        }

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
                        confirm = .revokeInvite(invite)
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

    // MARK: - Confirmation copy

    private var confirmTitle: String {
        guard let confirm else { return "" }
        switch confirm {
        case let .remove(_, isSelf): return isSelf ? "Leave Team" : "Remove Member"
        case let .changeRole(_, role): return role == DomainContract.teamRoleOwner ? "Make Owner" : "Make Member"
        case .revokeInvite: return "Revoke Invite"
        }
    }

    private func confirmMessage(_ c: MemberConfirm) -> String {
        switch c {
        case let .remove(member, isSelf):
            if isSelf {
                return "You will lose access to this team. An owner must invite you back."
            }
            let name = memberDisplayName(users.first { $0.id == member.userId }, id: member.userId)
            return "Remove \(name) from this team? They immediately lose access."
        case let .changeRole(member, role):
            let name = memberDisplayName(users.first { $0.id == member.userId }, id: member.userId)
            if role == DomainContract.teamRoleOwner {
                return "Make \(name) an owner? Owners can delete boards, manage members and billing, and delete the team."
            }
            return "Change \(name) to member? They will no longer be able to manage members, repositories, or delete boards."
        case .revokeInvite:
            return "The invite link stops working immediately."
        }
    }

    private func confirmButtonLabel(_ c: MemberConfirm) -> String {
        switch c {
        case let .remove(_, isSelf): return isSelf ? "Leave" : "Remove"
        case .changeRole: return "Change Role"
        case .revokeInvite: return "Revoke"
        }
    }

    private func isDestructive(_ c: MemberConfirm) -> Bool {
        switch c {
        case .remove, .revokeInvite: return true
        case .changeRole: return false
        }
    }

    // MARK: - Actions

    private func perform(_ c: MemberConfirm) async {
        do {
            switch c {
            case let .remove(member, _):
                try await membersApi.remove(accountId: accountId, memberId: member.id)
            case let .changeRole(member, role):
                try await membersApi.updateRole(accountId: accountId, memberId: member.id, role: role)
            case let .revokeInvite(invite):
                guard let api = invitesApi else { return }
                try await api.revoke(accountId: accountId, inviteId: invite.id)
            }
            actionError = nil
        } catch {
            actionError = error.trpcUserMessage
        }
        confirm = nil
    }

    private func generateLink(teamId: String, api: TeamInvitesApi) async {
        generating = true
        defer { generating = false }
        guard let base = WebLinks.normalizedBase(instanceBaseURL?.absoluteString) else {
            inviteError = "This server's URL is unavailable, so an invite link can't be built."
            return
        }
        do {
            let result = try await api.create(
                accountId: accountId, teamId: teamId, role: DomainContract.teamRoleMember
            )
            generatedLink = "\(base)/invite/\(result.token)"
            inviteError = nil
        } catch {
            inviteError = error.trpcUserMessage
        }
    }
}
