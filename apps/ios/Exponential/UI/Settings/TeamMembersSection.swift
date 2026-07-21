import ExpUI
import ExpCore
import SwiftUI

struct TeamMembersSection: View {
    let accountId: String
    let members: [TeamMemberEntity]
    let users: [UserEntity]
    let currentUserId: String?
    let membersApi: TeamMembersApi
    // Owner-only controls (role change / remove) are HIDDEN for non-owners —
    // full web parity, not greyed. Self-leave stays for anyone. Inviting
    // members is a web-only flow (EXP-216) — the app never offers it.
    var isOwner: Bool = false

    @State private var confirm: MemberConfirm?
    @State private var actionError: String?

    // Destructive/role actions are confirmed through a single alert.
    private enum MemberConfirm {
        case remove(TeamMemberEntity, isSelf: Bool)
        case changeRole(TeamMemberEntity, to: String)
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
        let displayName = memberDisplayName(user, id: member.userId)
        HStack(spacing: 12) {
            // Avatar (inert — there is no member-profile screen): the member's
            // photo when synced, else initials.
            UserAvatar(user: user, id: member.userId, size: 32)

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) {
                    Text(displayName)
                        .font(.subheadline)
                        .foregroundStyle(.white)
                    if isSelf {
                        Text("(you)")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
                // Skip the email sub-line when it IS the display name — a
                // name-less Apple user falls back to the email as the primary
                // line, and repeating it below would read as email-over-email.
                if let email = user?.email, !email.isEmpty, email != displayName {
                    Text(email)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }

            Spacer()

            // Role badge
            Text(member.role)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .glassButton()

            // Actions menu — only rendered when there is at least one action to
            // offer. Each action is a precomputed boolean, and the ellipsis is
            // hidden entirely when they are all false. "Make member" is HIDDEN
            // (not disabled) for the last owner, so the sole owner's own row —
            // whose only candidate action was that no-op — shows no menu at all.
            let canMakeOwner = isOwner && member.role != DomainContract.teamRoleOwner
            let canMakeMember = isOwner && member.role != DomainContract.teamRoleMember && !isLastOwner
            let canLeave = isSelf && !isLastOwner
            let canRemove = isOwner && !isSelf
            if canMakeOwner || canMakeMember || canLeave || canRemove {
                Menu {
                    if canMakeOwner {
                        Button {
                            confirm = .changeRole(member, to: DomainContract.teamRoleOwner)
                        } label: {
                            Label("Make owner", systemImage: "crown")
                        }
                    }
                    if canMakeMember {
                        Button {
                            confirm = .changeRole(member, to: DomainContract.teamRoleMember)
                        } label: {
                            Label("Make member", systemImage: "shield")
                        }
                    }
                    if canLeave {
                        Button(role: .destructive) {
                            confirm = .remove(member, isSelf: true)
                        } label: {
                            Label("Leave", systemImage: "xmark")
                        }
                    }
                    if canRemove {
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

    // MARK: - Confirmation copy

    private var confirmTitle: String {
        guard let confirm else { return "" }
        switch confirm {
        case let .remove(_, isSelf): return isSelf ? "Leave Team" : "Remove Member"
        case let .changeRole(_, role): return role == DomainContract.teamRoleOwner ? "Make Owner" : "Make Member"
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
                return "Make \(name) an owner? Owners can delete boards, manage members and repositories, and delete the team."
            }
            return "Change \(name) to member? They will no longer be able to manage members, repositories, or delete boards."
        }
    }

    private func confirmButtonLabel(_ c: MemberConfirm) -> String {
        switch c {
        case let .remove(_, isSelf): return isSelf ? "Leave" : "Remove"
        case .changeRole: return "Change Role"
        }
    }

    private func isDestructive(_ c: MemberConfirm) -> Bool {
        switch c {
        case .remove: return true
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
            }
            actionError = nil
        } catch {
            actionError = error.trpcUserMessage
        }
        confirm = nil
    }
}
