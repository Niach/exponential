import ExpUI
import ExpCore
import SwiftUI

struct TeamMembersSection: View {
    let accountId: String
    let teamId: String
    let members: [TeamMemberEntity]
    let users: [UserEntity]
    /// The team's synced invites — the "Invited" / "Invite expired" / "Not
    /// invited" badge (EXP-630 placeholder members, EXP-1076 import seats) and
    /// the owner's "Pending invites" rows (each with web's revoke trash).
    var invites: [TeamInviteEntity] = []
    let currentUserId: String?
    let membersApi: TeamMembersApi
    // Owner-only controls (role change / remove) are HIDDEN for non-owners —
    // full web parity, not greyed. Self-leave stays for anyone. Inviting is
    // owner-only too (EXP-725): owners get web's Invite members form (Send
    // invite · Add without inviting), the invite-LINK creator (only while the
    // team has free seats: a seat cap must never surface purchase copy in an
    // App Store build, 3.1.1) and the Pending invites list.
    var isOwner: Bool = false
    /// The team's name, for the Leave prompt's title.
    var teamName: String?

    @State private var confirm: MemberConfirm?
    @State private var inviteName = ""
    @State private var inviteEmail = ""
    @State private var sending = false
    @State private var adding = false
    /// A refused address (BAD_REQUEST) stays under the field.
    @State private var emailError: String?
    /// The link of an invite whose mail could not go out, shared by hand.
    @State private var undeliveredURL: URL?
    @State private var linkCopied = false
    @State private var revoking: Set<String> = []
    @Environment(\.toaster) private var toaster
    @Environment(AppDependencies.self) private var deps

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
                // Join order ×4 (web `membersInJoinOrder`): oldest first.
                ForEach(TeamMemberOrder.joinOrder(members), id: \.id) { member in
                    memberRow(member, placeholder: byMember[member.userId])
                }
            }

            if isOwner {
                Rectangle()
                    .fill(GlassTokens.strokeCard)
                    .frame(height: GlassTokens.hairline)
                    .padding(.vertical, 6)

                inviteForm

                // Web's outline "Generate invite link" (the hugging pill).
                InviteLinkCreator(accountId: accountId, teamId: teamId, embedded: true)

                // The still-open invites, web's "Pending invites" list in
                // web's order (newest invite first, then id; EXP-668).
                let pending = Self.webOrder(PendingInvites.pending(invites))
                if !pending.isEmpty {
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand("Pending invites")
                        ForEach(pending, id: \.id) { invite in
                            inviteRow(invite)
                        }
                    }
                    .padding(.top, 8)
                }
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
                // ×4: name · muted "(you)" · the role pill with its role
                // glyph right after the name, then the placeholder badge
                // (web members-section.tsx).
                HStack(spacing: 6) {
                    HStack(spacing: 4) {
                        Text(displayName)
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        if isSelf {
                            Text("(you)")
                                .font(.subheadline)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(1)
                                .fixedSize()
                        }
                    }
                    rolePill(member.role)
                    // EXP-630: an emailed invite puts its recipient on the
                    // roster at once, so a row can be a member who has not
                    // joined yet; EXP-1076: a row the Linear import seated
                    // without ever mailing reads "Not invited". Resending is
                    // a web surface (EXP-725), so the badge is all iOS shows.
                    if let placeholder {
                        GlassPill(
                            placeholder.label,
                            icon: AppIcons.uiMail,
                            tint: .white.opacity(TextOpacity.tertiary)
                        )
                        .fixedSize()
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

            Spacer(minLength: 0)

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

    /// The role pill ×4: the role glyph (owner crown, member shield) leading
    /// the role, member and invite rows alike.
    private func rolePill(_ role: String) -> some View {
        GlassPill(
            role,
            icon: role == DomainContract.teamRoleOwner ? AppIcons.uiOwner : AppIcons.uiMember
        )
        .fixedSize()
    }

    // MARK: - Invite form

    /// Web `InviteMemberForm` (Members settings): Name · email · Send invite
    /// (the primary, mail glyph) · Add without inviting (the outline, FEED-108).
    private var inviteForm: some View {
        let hasEmail = !inviteEmail.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        return VStack(alignment: .leading, spacing: 8) {
            GlassSectionBand("Invite members")
            GlassTextField("Name", text: $inviteName, accessibilityIdentifier: "invite-name")
                .textInputAutocapitalization(.words)
                .autocorrectionDisabled()
            GlassTextField("teammate@example.com", text: Binding(
                get: { inviteEmail },
                set: { inviteEmail = $0; emailError = nil }
            ), accessibilityIdentifier: "invite-email")
                .keyboardType(.emailAddress)
                .textContentType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.send)
                .onSubmit { if !sending && !adding { Task { await sendInvite() } } }
            if let emailError {
                Text(emailError)
                    .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
            Button { Task { await sendInvite() } } label: {
                GlassSubmitLabel(
                    "Send invite",
                    enabled: hasEmail && !adding,
                    loading: sending,
                    pendingWord: FeedbackStates.Pending.invite
                ) {
                    AppIcon(AppIcons.uiMail, size: AppIcon.Size.small)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.pressScale)
            .disabled(!hasEmail || adding || sending)
            .accessibilityIdentifier("invite-send")

            GlassOAuthButton("Add without inviting", action: { Task { await addWithoutInviting() } }) {
                AppIcon(AppIcons.uiInvite, size: AppIcon.Size.small)
            }
            .disabled(!hasEmail || sending || adding)
            .opacity(hasEmail && !sending ? 1 : 0.5)
            .accessibilityIdentifier("invite-add-without-inviting")

            if let undeliveredURL {
                undeliveredLinkRow(undeliveredURL)
            }
        }
    }

    /// Web `InviteLinkRow`: the read-only link + Copy.
    private func undeliveredLinkRow(_ url: URL) -> some View {
        HStack(spacing: 8) {
            Text(url.absoluteString)
                .font(.expMono(.caption))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .textSelection(.enabled)
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(maxWidth: .infinity, alignment: .leading)
            GhostIconButton(
                linkCopied ? AppIcons.uiCheck : AppIcons.uiCopy,
                accessibilityLabel: "Copy invite URL"
            ) {
                Platform.copyToPasteboard(url.absoluteString)
                linkCopied = true
                toaster.success("Invite link copied")
            }
        }
        .accessibilityIdentifier("invite-url")
    }

    private var trimmedEmail: String {
        inviteEmail.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var trimmedName: String? {
        let name = inviteName.trimmingCharacters(in: .whitespacesAndNewlines)
        return name.isEmpty ? nil : name
    }

    private func sendInvite() async {
        let to = trimmedEmail
        guard !to.isEmpty, !sending else { return }
        sending = true
        emailError = nil
        defer { sending = false }
        do {
            let result = try await deps.teamInvitesApi.create(
                accountId: accountId, teamId: teamId, email: to, name: trimmedName
            )
            if result.emailDelivered == true {
                toaster.success("Invite sent to \(to)")
                inviteName = ""
                inviteEmail = ""
                undeliveredURL = nil
            } else {
                let instanceUrl = deps.auth.accounts.first { $0.id == accountId }?.instanceUrl
                undeliveredURL = WebLinks.invite(instanceUrl: instanceUrl, token: result.token)
                linkCopied = false
                toaster.error("Couldn't email the invite. Copy the link below and share it instead.")
            }
        } catch {
            if error.isPlanLimitError {
                // App Store 3.1.1: no seat/purchase copy on iOS.
                toaster.error(FeedbackStates.Errors.title(verb: "send", thing: "the invite"))
            } else if error.trpcErrorCode == "BAD_REQUEST" {
                emailError = error.trpcUserMessage
            } else {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "send", thing: "the invite"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    private func addWithoutInviting() async {
        let to = trimmedEmail
        guard !to.isEmpty, !adding else { return }
        adding = true
        emailError = nil
        defer { adding = false }
        do {
            let name = try await deps.teamMembersApi.addPlaceholder(
                accountId: accountId, teamId: teamId, email: to, name: trimmedName
            )
            toaster.success("Added \(name) to the team")
            inviteName = ""
            inviteEmail = ""
            undeliveredURL = nil
        } catch {
            if error.trpcErrorCode == "BAD_REQUEST" {
                emailError = error.trpcUserMessage
            } else {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "add", thing: "the member"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    private func revoke(_ invite: TeamInviteEntity) async {
        revoking.insert(invite.id)
        defer { revoking.remove(invite.id) }
        do {
            try await deps.teamInvitesApi.revoke(accountId: accountId, id: invite.id)
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "revoke", thing: "the invite"),
                description: error.trpcUserMessage
            )
        }
    }

    /// Web `useTeamInvites`' order: newest invite first, then id.
    static func webOrder(_ invites: [TeamInviteEntity]) -> [TeamInviteEntity] {
        invites.sorted { a, b in
            let at = WireTimestamps.parse(a.createdAt) ?? .distantPast
            let bt = WireTimestamps.parse(b.createdAt) ?? .distantPast
            return at != bt ? at > bt : a.id < b.id
        }
    }

    // MARK: - Invite row

    /// ×4 anatomy: email or "Link invite" · role pill · "Expires Mon D"; a
    /// link invite wears the link glyph, an emailed one the mail glyph.
    private func inviteRow(_ invite: TeamInviteEntity) -> some View {
        let email = invite.email.flatMap { $0.isEmpty ? nil : $0 }
        return HStack(spacing: 8) {
            AppIcon(email == nil ? AppIcons.uiLink : AppIcons.uiMail, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text(email ?? PendingInvites.linkInviteLabel)
                .font(.subheadline)
                .foregroundStyle(email == nil ? Color.white.opacity(TextOpacity.tertiary) : Color.white)
                .lineLimit(1)
                .truncationMode(.middle)
            rolePill(invite.role)
            if let expiry = PendingInvites.expiryLabel(invite.expiresAt) {
                Text(expiry)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .fixedSize()
            }
            Spacer(minLength: 0)
            // Web's ghost trash: revoke at once (no confirm, like web).
            GhostIconButton(
                AppIcons.uiDelete,
                accessibilityLabel: "Revoke invite",
                enabled: !revoking.contains(invite.id)
            ) {
                Task { await revoke(invite) }
            }
            .accessibilityIdentifier("invite-revoke")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
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
            // FEED-79: `Couldn't <verb> <thing>` + the server's message.
            let title: String = switch c {
            case .remove: FeedbackStates.Errors.title(verb: "remove", thing: "the member")
            case .changeRole: FeedbackStates.Errors.title(verb: "change", thing: "the role")
            }
            toaster.error(title, description: error.trpcUserMessage)
        }
        confirm = nil
    }
}
