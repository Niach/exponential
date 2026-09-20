import ExpCore
import ExpUI
import GRDB
import SwiftUI

// EXP-920 — the entity chips under a settled Exponential tool row, and the
// preview sheet a chip opens. The contract's rule (`EntityPreview`, ExpCore)
// groups the row's `preview.refs` into chips; this file resolves each chip
// against THIS phone's synced rows (GRDB, the run's team) and decides where
// "Open" goes. ×4: web `entity-preview/`, desktop `steer::entity_preview`,
// Android `EntityRefChips`.

/// The chip a tap opened (`.sheet(item:)`).
struct EntityRefTarget: Identifiable {
    let group: EntityPreview.Group
    var id: String { "\(group.ref.kind):\(group.ref.id)" }
}

/// Where a preview's "Open" (or a member row's tap) goes — resolved into a
/// route by the chips view, which owns the navigation seams.
enum EntityNavigation: Equatable {
    case issue(String)
    case board(String)
    case session(String)
    case workflow(String)
    case thread(String)
    case actions
    case devices
    case teamSettings
    case myWork
}

/// The wrapping chip row: one `EntityChip` per group, a tap opens the
/// preview sheet. Sits under the tool row's caption, indented like the
/// single issue chip did before EXP-920.
struct EntityRefChips: View {
    let refs: [EntityRef]
    let context: AgentIssueRefContext?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @State private var target: EntityRefTarget?

    private var groups: [EntityPreview.Group] { EntityPreview.groupRefs(refs) }

    var body: some View {
        FlowLayout(spacing: 6) {
            ForEach(Array(groups.enumerated()), id: \.offset) { _, group in
                EntityRefChip(group: group, context: context) {
                    target = EntityRefTarget(group: group)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .sheet(item: $target) { target in
            EntityRefPreviewSheet(
                group: target.group,
                context: context,
                accountId: accountId,
                onNavigate: { destination in
                    self.target = nil
                    navigate(destination)
                }
            )
        }
    }

    private func navigate(_ destination: EntityNavigation) {
        switch destination {
        case let .issue(id):
            if let context {
                context.onOpen(id)
            } else {
                deps.deepLinkBus.navigateToIssue(id, accountId: accountId)
            }
        case let .board(id):
            pushRoute(.board(accountId: accountId, id: id))
        case let .session(id):
            pushRoute(.agentSession(accountId: accountId, sessionId: id))
        case let .workflow(id):
            pushRoute(.workflow(accountId: accountId, id: id))
        case let .thread(id):
            pushRoute(.supportThread(accountId: accountId, threadId: id))
        case .actions:
            pushRoute(.actions)
        case .devices:
            pushRoute(.agents)
        case .teamSettings:
            if let context {
                pushRoute(.teamSettings(accountId: accountId, teamId: context.teamId))
            } else {
                pushRoute(.settings)
            }
        case .myWork:
            pushRoute(.myWork)
        }
    }
}

/// One chip. An issue that has synced wears its resolved status glyph and
/// the store's title (through the editors' chip memo, so a brand-new issue
/// chips within seconds of arriving); anything else the concept glyph and
/// the contract label.
private struct EntityRefChip: View {
    let group: EntityPreview.Group
    let context: AgentIssueRefContext?
    let onTap: () -> Void

    @Environment(\.accountId) private var accountId

    @MainActor
    private var resolvedIssue: IssueRefLookup.Chip? {
        guard group.ref.kind == "issue", let context,
              let identifier = group.ref.identifier, !identifier.isEmpty
        else { return nil }
        return IssueRefChipCache.chip(
            identifier, scope: .team(id: context.teamId), db: context.db, accountId: accountId
        )
    }

    var body: some View {
        let ref = group.ref
        if let chip = resolvedIssue, let identifier = ref.identifier {
            EntityChip(
                label: EntityPreview.clampChipLabel(identifier),
                detail: EntityPreview.clampChipLabel(chip.title),
                iconName: chip.status.iconName,
                iconColor: chip.status.color,
                bodySize: DesignTokens.Transcript.bodySize,
                onTap: onTap
            )
        } else {
            EntityChip(
                ref: ref,
                bodySize: DesignTokens.Transcript.bodySize,
                onTap: onTap
            )
        }
    }
}

// MARK: - The preview sheet

/// What the sheet paints, resolved once per presentation off the synced rows.
struct EntityPreviewModel {
    enum Icon {
        case glyph(String, Color?)
        case user(UserEntity?, id: String?)
        case team(TeamEntity)
        case sessionDot(SessionDotTone, pulsing: Bool)
        case dot(Color)
    }

    struct Fact: Identifiable {
        let id: String
        let label: String
        var icon: String? = nil
        var dot: Color? = nil
        var tint: Color? = nil
    }

    struct Row: Identifiable {
        let id: String
        var icon: Icon? = nil
        let primary: String
        var secondary: String? = nil
        var open: EntityNavigation? = nil
    }

    var icon: Icon
    var eyebrow: String?
    var title: String
    var subtitle: String?
    var excerpt: String?
    var facts: [Fact] = []
    var rows: [Row] = []
    var more: String?
    var open: EntityNavigation?
}

struct EntityRefPreviewSheet: View {
    let group: EntityPreview.Group
    let context: AgentIssueRefContext?
    let accountId: String
    let onNavigate: (EntityNavigation) -> Void

    var body: some View {
        let model = EntityRefResolver.resolve(
            group: group, teamId: context?.teamId, db: context?.db, accountId: accountId
        )
        EntityPreviewCard(
            eyebrow: model.eyebrow,
            title: model.title,
            subtitle: model.subtitle,
            excerpt: model.excerpt,
            rows: model.rows.map { row in
                EntityPreviewRow(
                    id: row.id,
                    icon: row.icon.map { AnyView(iconView($0, size: 14)) },
                    primary: row.primary,
                    secondary: row.secondary,
                    onTap: row.open.map { destination in { onNavigate(destination) } }
                )
            },
            more: model.more,
            onOpen: model.open.map { destination in { onNavigate(destination) } },
            icon: { iconView(model.icon, size: 18) },
            facts: {
                ForEach(model.facts) { fact in
                    if let icon = fact.icon {
                        GlassPill(fact.label, icon: icon, dot: fact.dot, tint: fact.tint)
                    } else {
                        GlassPill(fact.label, dot: fact.dot, tint: fact.tint)
                    }
                }
            }
        )
    }

    @ViewBuilder
    private func iconView(_ icon: EntityPreviewModel.Icon, size: CGFloat) -> some View {
        switch icon {
        case let .glyph(name, color):
            AppIcon(name, size: size)
                .foregroundStyle(color ?? .white.opacity(TextOpacity.secondary))
        case let .user(user, id):
            UserAvatar(user: user, id: id, size: size + 6)
        case let .team(team):
            TeamAvatar(team: team, size: size + 6)
        case let .sessionDot(tone, pulsing):
            SessionStateDot(tone: tone, pulsing: pulsing, size: size * 0.6)
        case let .dot(color):
            Circle().fill(color).frame(width: size * 0.6, height: size * 0.6)
        }
    }
}

// MARK: - Resolution

/// One synchronous read per presentation: the sheet is short-lived, and the
/// rows it lists are a handful. Every branch degrades to the SLIM card — the
/// ref's own label under the kind's glyph — when the row has not synced.
@MainActor
enum EntityRefResolver {
    static func resolve(
        group: EntityPreview.Group, teamId: String?, db: DatabaseManager?, accountId: String
    ) -> EntityPreviewModel {
        let ref = group.ref
        let slim = slimModel(ref)
        guard let db, let pool = try? db.pool(forAccountId: accountId) else { return slim }
        if ref.kind == "list" {
            return listModel(group, pool: pool, teamId: teamId) ?? slim
        }
        let resolved: EntityPreviewModel? = try? pool.read { db in
            switch ref.kind {
            case "issue": try issueModel(ref, db: db, teamId: teamId)
            case "board": try boardModel(ref, db: db)
            case "action": try actionModel(ref, db: db)
            case "automation": try automationModel(ref, db: db)
            case "comment": try commentModel(ref, db: db, teamId: teamId)
            case "session": try sessionModel(ref, db: db)
            case "label": try labelModel(ref, db: db)
            case "status": try statusModel(ref, db: db)
            case "workflow": try workflowModel(ref, db: db)
            case "device": try deviceModel(ref, db: db)
            case "member": try memberModel(ref, db: db, teamId: teamId)
            case "team": try teamModel(ref, db: db)
            case "invite": try inviteModel(ref, db: db)
            case "notification": try notificationModel(ref, db: db)
            case "attachment": try attachmentModel(ref, db: db, teamId: teamId)
            default: nil
            }
        }
        return resolved ?? slim
    }

    /// The card for a ref this phone cannot resolve: the kind's glyph, the
    /// noun as the eyebrow, the chip label as the title. Open only where the
    /// destination is a LIST page that needs no row of its own.
    private static func slimModel(_ ref: EntityRef) -> EntityPreviewModel {
        let glyph = EntityChipIcon.glyph(for: ref)
        let noun = ref.kind == "list"
            ? EntityPreview.kindNoun(ref.id, count: max(0, ref.count ?? 0))
            : EntityPreview.kindNoun(ref.kind)
        return EntityPreviewModel(
            icon: .glyph(glyph, nil),
            eyebrow: ref.kind == "issue" && ref.identifier != nil ? nil : capitalized(noun),
            title: EntityPreview.chipLabel(ref),
            subtitle: EntityPreview.chipDetail(ref),
            open: slimOpen(ref)
        )
    }

    private static func slimOpen(_ ref: EntityRef) -> EntityNavigation? {
        switch ref.kind {
        case "action", "automation": .actions
        case "label", "status", "member", "invite", "team", "repository": .teamSettings
        case "device": .devices
        case "notification": .myWork
        case "thread": .thread(ref.id)
        default: nil
        }
    }

    // MARK: Kinds

    private static func issueModel(_ ref: EntityRef, db: Database, teamId: String?) throws -> EntityPreviewModel? {
        var issue = try IssueEntity.fetchOne(db, key: ref.id)
        if issue == nil, let identifier = ref.identifier, let teamId {
            issue = try IssueEntity.fetchOne(
                db,
                sql: """
                SELECT i.* FROM issues i JOIN boards p ON p.id = i.board_id
                WHERE upper(i.identifier) = ? AND p.team_id = ?
                """,
                arguments: [identifier.uppercased(), teamId]
            )
        }
        guard let issue else { return nil }
        let board = try BoardEntity.fetchOne(db, key: issue.boardId)
        let status = try resolvedStatus(issue, db: db, teamId: board?.teamId ?? teamId)
        var facts: [EntityPreviewModel.Fact] = [
            .init(id: "status", label: status.name, icon: status.iconName, tint: status.color),
        ]
        let priority = IssuePriority(rawValue: issue.priority) ?? .none
        if priority != .none {
            facts.append(.init(id: "priority", label: priority.label, icon: priority.iconName, tint: priority.color))
        }
        let labelIds = try IssueLabelEntity.filter(Column("issue_id") == issue.id).fetchAll(db).map(\.labelId)
        if !labelIds.isEmpty {
            let labels = try LabelEntity.filter(labelIds.contains(Column("id"))).order(Column("name")).fetchAll(db)
            for label in labels {
                facts.append(.init(id: "label:\(label.id)", label: label.name, dot: Color(hex: label.color)))
            }
        }
        var subtitle = board?.name
        if let assigneeId = issue.assigneeId {
            let assignee = try UserEntity.fetchOne(db, key: assigneeId)
            let name = memberDisplayName(assignee, id: assigneeId)
            subtitle = [subtitle, "Assigned to \(name)"].compactMap { $0 }.joined(separator: " · ")
        }
        return EntityPreviewModel(
            icon: .glyph(status.iconName, status.color),
            eyebrow: issue.identifier ?? "Issue",
            title: issue.title.isEmpty ? "Untitled issue" : issue.title,
            subtitle: subtitle,
            facts: facts,
            open: .issue(issue.id)
        )
    }

    private static func boardModel(_ ref: EntityRef, db: Database) throws -> EntityPreviewModel? {
        guard let board = try BoardEntity.fetchOne(db, key: ref.id) else { return nil }
        let closed = [IssueStatus.done, .cancelled, .duplicate].map(\.rawValue)
        let open = IssueEntity
            .filter(Column("board_id") == board.id)
            .filter(!closed.contains(Column("status")))
        let total = try open.fetchCount(db)
        let issues = try open.order(Column("updated_at").desc).limit(6).fetchAll(db)
        let statuses = try teamStatuses(db: db, teamId: board.teamId)
        let rows = issues.map { issue in
            let status = IssueStatusResolver.resolve(issue, team: statuses)
            return EntityPreviewModel.Row(
                id: issue.id,
                icon: .glyph(status.iconName, status.color),
                primary: issue.title.isEmpty ? "Untitled issue" : issue.title,
                secondary: issue.identifier,
                open: .issue(issue.id)
            )
        }
        return EntityPreviewModel(
            icon: .glyph(BoardTypeDisplay.iconName(for: board), Color(hex: board.color)),
            eyebrow: "Board",
            title: board.name,
            subtitle: "\(board.prefix) · \(total) open \(total == 1 ? "issue" : "issues")",
            rows: rows,
            more: total > rows.count ? "+\(total - rows.count) more" : nil,
            open: .board(board.id)
        )
    }

    private static func actionModel(_ ref: EntityRef, db: Database) throws -> EntityPreviewModel? {
        guard let action = try ActionEntity.fetchOne(db, key: ref.id) else { return nil }
        var facts: [EntityPreviewModel.Fact] = []
        if action.repositoryId != nil {
            facts.append(.init(id: "repo", label: "Repository", icon: AppIcons.uiRepository))
        }
        let inputs = action.inputs.flatMap { try? JSONDecoder().decode([ActionInputDto].self, from: Data($0.utf8)) } ?? []
        facts.append(.init(id: "inputs", label: "\(inputs.count) \(inputs.count == 1 ? "input" : "inputs")"))
        return EntityPreviewModel(
            icon: .glyph(ActionIconDisplay.iconName(for: action.icon), nil),
            eyebrow: "Action",
            title: action.name,
            excerpt: action.description,
            facts: facts,
            open: .actions
        )
    }

    private static func automationModel(_ ref: EntityRef, db: Database) throws -> EntityPreviewModel? {
        guard let automation = try AutomationEntity.fetchOne(db, key: ref.id) else { return nil }
        let action = try ActionEntity.fetchOne(db, key: automation.actionId)
        let device = try DeviceEntity
            .filter(Column("device_id") == automation.deviceId || Column("id") == automation.deviceId)
            .fetchOne(db)
        var facts: [EntityPreviewModel.Fact] = [
            automation.enabled
                ? .init(id: "enabled", label: "Enabled", tint: DesignTokens.Semantic.green)
                : .init(id: "enabled", label: "Disabled"),
        ]
        if let device {
            facts.append(.init(id: "device", label: device.label, icon: DeviceIconDisplay.iconName(for: device)))
        }
        let trigger = AutomationTrigger.parse(automation.trigger).map(AutomationTriggerDisplay.summary)
        return EntityPreviewModel(
            icon: .glyph(AppIcons.navAutomations, nil),
            eyebrow: "Automation",
            title: ref.title ?? action?.name ?? "Automation",
            subtitle: [action.map { "Runs \($0.name)" }, trigger].compactMap { $0 }.joined(separator: " · "),
            facts: facts,
            open: .actions
        )
    }

    private static func commentModel(_ ref: EntityRef, db: Database, teamId: String?) throws -> EntityPreviewModel? {
        guard let comment = try CommentEntity.fetchOne(db, key: ref.id) else { return nil }
        let author = try UserEntity.fetchOne(db, key: comment.authorId)
        let issue = try IssueEntity.fetchOne(db, key: comment.issueId)
        var rows: [EntityPreviewModel.Row] = []
        if let issue {
            let status = try resolvedStatus(issue, db: db, teamId: teamId ?? comment.teamId)
            rows.append(.init(
                id: issue.id,
                icon: .glyph(status.iconName, status.color),
                primary: issue.title.isEmpty ? "Untitled issue" : issue.title,
                secondary: issue.identifier,
                open: .issue(issue.id)
            ))
        }
        return EntityPreviewModel(
            icon: .user(author, id: comment.authorId),
            eyebrow: comment.isViaMcp ? "Comment · via MCP" : "Comment",
            title: memberDisplayName(author, id: comment.authorId),
            subtitle: relativeDate(comment.createdAt),
            excerpt: comment.body,
            rows: rows,
            open: .issue(comment.issueId)
        )
    }

    private static func sessionModel(_ ref: EntityRef, db: Database) throws -> EntityPreviewModel? {
        guard let session = try CodingSessionEntity.fetchOne(db, key: ref.id) else { return nil }
        let issue = try session.issueId.flatMap { try IssueEntity.fetchOne(db, key: $0) }
        let batchIssues: [IssueEntity] = try {
            let ids = BatchRun.issueIds(session.batchIssueIds)
            guard !ids.isEmpty else { return [] }
            return try IssueEntity.filter(ids.contains(Column("id"))).fetchAll(db)
        }()
        let state = CodingSessionDisplayState.of(session: session, prState: issue?.prState ?? session.prState)
        let devices = try DeviceEntity.fetchAll(db)
        let device = SessionDevicePresentation.resolve(session: session, devices: devices)
        var facts: [EntityPreviewModel.Fact] = [
            .init(id: "state", label: stateLabel(state), tint: SessionStateDot.color(SessionStateDot.tone(of: state))),
        ]
        if let agent = session.agent, !agent.isEmpty {
            facts.append(.init(id: "agent", label: agent.capitalized))
        }
        if let label = device.label, !label.isEmpty {
            facts.append(.init(id: "device", label: label, icon: AppIcons.uiDevice))
        }
        return EntityPreviewModel(
            icon: .sessionDot(
                CodingSessionLiveness.isLive(session) ? SessionStateDot.tone(of: state) : .muted,
                pulsing: session.agentBusy
            ),
            eyebrow: sessionRowIdentifier(issue: issue, session: session, batchIssues: batchIssues) ?? "Run",
            title: sessionRowTitle(issue: issue, session: session, batchIssues: batchIssues),
            subtitle: "Started \(relativeDate(session.startedAt))",
            facts: facts,
            open: .session(session.id)
        )
    }

    private static func labelModel(_ ref: EntityRef, db: Database) throws -> EntityPreviewModel? {
        guard let label = try LabelEntity.fetchOne(db, key: ref.id) else { return nil }
        let color = Color(hex: label.color) ?? .white.opacity(TextOpacity.secondary)
        let count = try IssueLabelEntity.filter(Column("label_id") == label.id).fetchCount(db)
        return EntityPreviewModel(
            icon: .dot(color),
            eyebrow: "Label",
            title: label.name,
            subtitle: "\(count) \(count == 1 ? "issue" : "issues")",
            open: .teamSettings
        )
    }

    private static func statusModel(_ ref: EntityRef, db: Database) throws -> EntityPreviewModel? {
        guard let row = try IssueStatusEntity.fetchOne(db, key: ref.id) else { return nil }
        let statuses = try teamStatuses(db: db, teamId: row.teamId)
        let status = IssueStatusResolver.resolve(statusId: row.id, anchor: row.builtinKey, team: statuses)
        return EntityPreviewModel(
            icon: .glyph(status.iconName, status.color),
            eyebrow: "Status",
            title: status.name,
            subtitle: categoryLabel(status.category),
            open: .teamSettings
        )
    }

    private static func workflowModel(_ ref: EntityRef, db: Database) throws -> EntityPreviewModel? {
        guard let workflow = try WorkflowEntity.fetchOne(db, key: ref.id) else { return nil }
        let nodes = try WorkflowNodeEntity.filter(Column("workflow_id") == workflow.id).fetchCount(db)
        return EntityPreviewModel(
            icon: .glyph(AppIcons.navWorkflows, nil),
            eyebrow: "Workflow",
            title: workflow.name,
            subtitle: workflow.status.capitalized,
            facts: [.init(id: "nodes", label: "\(nodes) \(nodes == 1 ? "node" : "nodes")")],
            open: .workflow(workflow.id)
        )
    }

    private static func deviceModel(_ ref: EntityRef, db: Database) throws -> EntityPreviewModel? {
        guard let device = try DeviceEntity
            .filter(Column("id") == ref.id || Column("device_id") == ref.id)
            .fetchOne(db)
        else { return nil }
        let online = DeviceLiveness.isOnline(lastSeenAt: device.lastSeenAt)
        return EntityPreviewModel(
            icon: .glyph(DeviceIconDisplay.iconName(for: device), nil),
            eyebrow: "Device",
            title: device.label.isEmpty ? (ref.title ?? "Device") : device.label,
            subtitle: [device.platform, device.version].compactMap { $0 }.joined(separator: " · "),
            facts: [
                online
                    ? .init(id: "online", label: "Online", tint: DesignTokens.Semantic.green)
                    : .init(id: "online", label: "Offline"),
            ],
            open: .devices
        )
    }

    private static func memberModel(_ ref: EntityRef, db: Database, teamId: String?) throws -> EntityPreviewModel? {
        let user = try UserEntity.fetchOne(db, key: ref.id)
        var membership: TeamMemberEntity?
        if let teamId {
            membership = try TeamMemberEntity
                .filter(Column("team_id") == teamId && Column("user_id") == ref.id)
                .fetchOne(db)
        }
        guard user != nil || membership != nil else { return nil }
        return EntityPreviewModel(
            icon: .user(user, id: ref.id),
            eyebrow: "Member",
            title: memberDisplayName(user, id: ref.id),
            subtitle: user?.email,
            facts: membership.map { [.init(id: "role", label: $0.role.capitalized)] } ?? [],
            open: .teamSettings
        )
    }

    private static func teamModel(_ ref: EntityRef, db: Database) throws -> EntityPreviewModel? {
        guard let team = try TeamEntity.fetchOne(db, key: ref.id) else { return nil }
        let members = try TeamMemberEntity.filter(Column("team_id") == team.id).fetchCount(db)
        return EntityPreviewModel(
            icon: .team(team),
            eyebrow: "Team",
            title: team.name,
            subtitle: "\(members) \(members == 1 ? "member" : "members")",
            open: .teamSettings
        )
    }

    private static func inviteModel(_ ref: EntityRef, db: Database) throws -> EntityPreviewModel? {
        guard let invite = try TeamInviteEntity.fetchOne(db, key: ref.id) else { return nil }
        var facts: [EntityPreviewModel.Fact] = [.init(id: "role", label: invite.role.capitalized)]
        if invite.acceptedAt != nil {
            facts.append(.init(id: "accepted", label: "Accepted", tint: DesignTokens.Semantic.green))
        }
        return EntityPreviewModel(
            icon: .glyph(AppIcons.uiInvite, nil),
            eyebrow: "Invite",
            title: invite.email ?? ref.title ?? "Invite link",
            subtitle: "Expires \(relativeDate(invite.expiresAt))",
            facts: facts,
            open: .teamSettings
        )
    }

    private static func notificationModel(_ ref: EntityRef, db: Database) throws -> EntityPreviewModel? {
        guard let notification = try NotificationEntity.fetchOne(db, key: ref.id) else { return nil }
        return EntityPreviewModel(
            icon: .glyph(AppIcons.navNotifications, nil),
            eyebrow: "Notification",
            title: notification.title,
            subtitle: relativeDate(notification.createdAt),
            excerpt: notification.body,
            open: notification.issueId.map { .issue($0) } ?? .myWork
        )
    }

    private static func attachmentModel(_ ref: EntityRef, db: Database, teamId: String?) throws -> EntityPreviewModel? {
        guard let attachment = try AttachmentEntity.fetchOne(db, key: ref.id) else { return nil }
        let issue = try IssueEntity.fetchOne(db, key: attachment.issueId)
        var rows: [EntityPreviewModel.Row] = []
        if let issue {
            let status = try resolvedStatus(issue, db: db, teamId: teamId ?? attachment.teamId)
            rows.append(.init(
                id: issue.id,
                icon: .glyph(status.iconName, status.color),
                primary: issue.title.isEmpty ? "Untitled issue" : issue.title,
                secondary: issue.identifier,
                open: .issue(issue.id)
            ))
        }
        return EntityPreviewModel(
            icon: .glyph(AppIcons.uiAttach, nil),
            eyebrow: "Attachment",
            title: attachment.filename,
            subtitle: "\(Int64(attachment.sizeBytes).formatted(.byteCount(style: .file))) · \(attachment.contentType)",
            rows: rows,
            open: .issue(attachment.issueId)
        )
    }

    /// A `list` chip: the chip label as the title, every absorbed member as a
    /// row resolved like a mini chip, and the `+N more` the publisher's count
    /// says lie beyond the refs it sent.
    private static func listModel(
        _ group: EntityPreview.Group, pool: DatabasePool, teamId: String?
    ) -> EntityPreviewModel? {
        let ref = group.ref
        let count = max(0, ref.count ?? 0)
        let rows: [EntityPreviewModel.Row] = (try? pool.read { db in
            try group.members.map { member in try memberRow(member, db: db, teamId: teamId) }
        }) ?? group.members.map { memberRow($0) }
        return EntityPreviewModel(
            icon: .glyph(EntityChipIcon.glyph(for: ref), nil),
            eyebrow: "List",
            title: EntityPreview.chipLabel(ref),
            rows: rows,
            more: count > rows.count ? "+\(count - rows.count) more" : nil
        )
    }

    /// A member row off the store when it is there, else the ref's own label.
    private static func memberRow(_ ref: EntityRef, db: Database, teamId: String?) throws -> EntityPreviewModel.Row {
        if ref.kind == "issue" {
            var issue = try IssueEntity.fetchOne(db, key: ref.id)
            if issue == nil, let identifier = ref.identifier, let teamId {
                issue = try IssueEntity.fetchOne(
                    db,
                    sql: """
                    SELECT i.* FROM issues i JOIN boards p ON p.id = i.board_id
                    WHERE upper(i.identifier) = ? AND p.team_id = ?
                    """,
                    arguments: [identifier.uppercased(), teamId]
                )
            }
            if let issue {
                let status = try resolvedStatus(issue, db: db, teamId: teamId)
                return .init(
                    id: issue.id,
                    icon: .glyph(status.iconName, status.color),
                    primary: issue.title.isEmpty ? "Untitled issue" : issue.title,
                    secondary: issue.identifier,
                    open: .issue(issue.id)
                )
            }
        }
        return memberRow(ref)
    }

    private static func memberRow(_ ref: EntityRef) -> EntityPreviewModel.Row {
        .init(
            id: "\(ref.kind):\(ref.id)",
            icon: .glyph(EntityChipIcon.glyph(for: ref), nil),
            primary: EntityPreview.chipLabel(ref),
            secondary: EntityPreview.chipDetail(ref),
            open: memberOpen(ref)
        )
    }

    /// Where a member row goes: an entity with a detail screen of its own
    /// opens it by id, the settings-bound kinds their list page.
    private static func memberOpen(_ ref: EntityRef) -> EntityNavigation? {
        switch ref.kind {
        case "issue": .issue(ref.id)
        case "board": .board(ref.id)
        case "session": .session(ref.id)
        case "workflow": .workflow(ref.id)
        case "comment", "attachment": nil
        default: slimOpen(ref)
        }
    }

    // MARK: Helpers

    private static func teamStatuses(db: Database, teamId: String?) throws -> [ResolvedIssueStatus] {
        guard let teamId else { return IssueStatusResolver.teamStatusesOrFallback([]) }
        let rows = try IssueStatusEntity.filter(Column("team_id") == teamId).fetchAll(db)
        return IssueStatusResolver.teamStatusesOrFallback(rows)
    }

    private static func resolvedStatus(_ issue: IssueEntity, db: Database, teamId: String?) throws -> ResolvedIssueStatus {
        let scope = try teamId ?? String.fetchOne(
            db, sql: "SELECT team_id FROM boards WHERE id = ?", arguments: [issue.boardId]
        )
        return IssueStatusResolver.resolve(issue, team: try teamStatuses(db: db, teamId: scope))
    }

    private static func stateLabel(_ state: CodingSessionDisplayState) -> String {
        switch state {
        case .running: "Running"
        case .needsInput: "Needs input"
        case .review: "In review"
        case .done: "Done"
        }
    }

    private static func categoryLabel(_ category: IssueStatusCategory) -> String {
        switch category {
        case .backlog: "Backlog"
        case .unstarted: "Unstarted"
        case .started: "Started"
        case .completed: "Completed"
        case .cancelled: "Cancelled"
        case .duplicate: "Duplicate"
        }
    }

    private static func capitalized(_ text: String) -> String {
        guard let first = text.first else { return text }
        return first.uppercased() + text.dropFirst()
    }

    private static func relativeDate(_ wire: String) -> String {
        guard let date = WireTimestamps.parse(wire) else { return "" }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}
