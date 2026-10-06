import ExpUI
import ExpCore
import SwiftUI

struct TeamMembersSection: View {
    let accountId: String
    let teamId: String
    let members: [TeamMemberEntity]
    let users: [UserEntity]
    /// The team's synced invites — read for the "Invited" / "Invite expired" /
    /// "Not invited" badge alone (EXP-630 placeholder members, EXP-1076 import
    /// seats). Minting and resending stay web-only surfaces.
    var invites: [TeamInviteEntity] = []
    let currentUserId: String?
    let membersApi: TeamMembersApi
    // Owner-only controls (role change / remove) are HIDDEN for non-owners —
    // full web parity, not greyed. Self-leave stays for anyone. Inviting is
    // owner-only too (EXP-725): owners get the invite-LINK creator under the
    // rows — and only while the team has free seats, since a seat cap must
    // never surface purchase copy in an App Store build (3.1.1). Emailed
    // invites stay web-only.
    var isOwner: Bool = false
    /// The team's name, for the Leave prompt's title.
    var teamName: String?

    @State private var confirm: MemberConfirm?
    @Environment(\.toaster) private var toaster

    // Destructive/role actions are confirmed through a single alert.
    private enum MemberConfirm {
        case remove(TeamMemberEntity, isSelf: Bool)
        case changeRole(TeamMemberEntity, to: String)
    }

    // A team must always keep at least one owner.
    private var ownerCount: Int {
        members.filter { $0.role == DomainContract.teamRoleOwner }.count
    }

    // EXP-630: member id → "invited, not joined" state, folded ONCE per render
    // rather than per row.
    private var placeholders: [String: PlaceholderStatus] {
        placeholderStatuses(invites: invites)
    }

    var body: some View {
        // EXP-818: the filled group band over flat rows (Boards/Labels
        // parity) — one table, not a stack of cards. The error caption and the
        // invite creator below keep their own spacing.
        VStack(alignment: .leading, spacing: 10) {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand("Members")

                let byMember = placeholders
                ForEach(members, id: \.id) { member in
                    memberRow(member, placeholder: byMember[member.userId])
                }
            }

            if isOwner {
                InviteLinkCreator(accountId: accountId, teamId: teamId)
            }
        }
        // EXP-1215: the app's own alert card (`GlassAlert`), ×4.
        .glassAlert(item: $confirm) { target in
            let copy = prompt(target)
            // The ONE answer that is not Cancel (Remove, Leave, Make owner,
            // Make member) performs the confirmed change.
            let answer = copy.actions.first { $0.role != .cancel }?.id ?? ""
            return GlassAlert(prompt: copy, handlers: [answer: { Task { await perform(target) } }])
        }
    }

    // MARK: - Member row

    @ViewBuilder
    private func memberRow(
        _ member: TeamMemberEntity, placeholder: PlaceholderStatus?
    ) -> some View {
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
            GlassPill(member.role)

            // EXP-630: an emailed invite puts its recipient on the roster at
            // once, so a row can be a member who has not joined yet. The badge
            // says which — muted, right after the role, web parity
            // (members-section.tsx); EXP-1076: a row the Linear import seated
            // without ever mailing reads "Not invited". Resending is a web
            // surface (EXP-725), so the badge is all iOS shows.
            if let placeholder {
                GlassPill(
                    placeholder.label,
                    icon: AppIcons.uiMail,
                    tint: .white.opacity(TextOpacity.tertiary)
                )
            }

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
                GlassMenu {
                    if canMakeOwner {
                        GlassMenuItem("Make owner", icon: AppIcons.uiOwner) {
                            confirm = .changeRole(member, to: DomainContract.teamRoleOwner)
                        }
                    }
                    if canMakeMember {
                        GlassMenuItem("Make member", icon: AppIcons.uiMember) {
                            confirm = .changeRole(member, to: DomainContract.teamRoleMember)
                        }
                    }
                    if canLeave {
                        GlassMenuItem("Leave", icon: AppIcons.navSignOut, destructive: true) {
                            confirm = .remove(member, isSelf: true)
                        }
                    }
                    if canRemove {
                        GlassMenuItem("Remove", icon: AppIcons.uiRemoveMember, destructive: true) {
                            confirm = .remove(member, isSelf: false)
                        }
                    }
                } label: {
                    GhostIconLabel(AppIcons.uiMore)
                        .accessibilityLabel("Member actions")
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .flatRow()
    }

    // MARK: - Confirmation copy

    /// EXP-1215: the contract prompt for a confirm (`prompts.json`).
    private func prompt(_ c: MemberConfirm) -> PromptCopy {
        switch c {
        case let .remove(member, isSelf):
            if isSelf { return Prompts.LeaveTeam.copy(name: teamName ?? "this team") }
            return Prompts.RemoveMember.copy(name: name(of: member))
        case let .changeRole(member, role):
            if role == DomainContract.teamRoleOwner {
                return Prompts.MakeOwner.copy(name: name(of: member))
            }
            return Prompts.MakeMember.copy(name: name(of: member))
        }
    }

    private func name(of member: TeamMemberEntity) -> String {
        memberDisplayName(users.first { $0.id == member.userId }, id: member.userId)
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
        } catch {
            toaster.error(error.trpcUserMessage)
        }
        confirm = nil
    }
}
