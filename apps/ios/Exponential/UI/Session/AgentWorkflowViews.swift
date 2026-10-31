import ExpCore
import ExpUI
import GRDB
import SwiftUI

// EXP-850 — the three transcript surfaces the steer wire's new slots draw:
// the working row (§5), the strip above the composer (§1/§2) and the workflow
// card (§3) with its duplicate warning (§4/EXP-856).
//
// Their own file on purpose: `AgentSessionView` is at the type checker's
// budget (#644, #656), and every condition spelled out inside one of its
// closures is type-checked as part of that one expression.

/// The trailing "agent is busy" row (EXP-389): a gently pulsing caption under
/// the newest event whenever the session is live and nothing waits on the user
/// — without it a feed that ends in tool rows gives no cue whether the agent is
/// still going.
///
/// EXP-850 §5: the caption is the turn's own verb with its duration and token
/// count (`Weaving… (2m 04s · ↓ 12.4k tokens)`), or the running workflow's §7
/// caption while one runs, and the glyph beside it is the RUNNING AGENT's
/// working mark (EXP-1184: Claude's spark, else the pulsing brand mark). The
/// mark is what moves; the text stays put, so a caption that ticks every
/// second is still readable. Static under Reduce Motion.
struct WorkingIndicatorRow: View {
    /// The run's coding agent (contract `codingAgent`), for the brand mark.
    /// Nil is a claude run (web parity); an id outside the contract draws
    /// the neutral agents glyph.
    var agent: String?
    /// §5's caption at `now`, re-derived by the caller on every tick.
    let caption: (Date) -> String

    var body: some View {
        // The caption carries a clock, so the row re-renders once a second.
        // A `TimelineView` is the one thing that ticks without a Task per row.
        TimelineView(.periodic(from: .now, by: 1)) { context in
            HStack(spacing: 8) {
                mark
                Text(caption(context.date))
                    .transcriptToolText()
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("agent-working-row")
    }

    /// EXP-1184: the agent's working mark — Claude's frame-stepped spark,
    /// else the brand mark with the EXP-850 beat (`AgentWorkingMark`).
    private var mark: some View {
        AgentWorkingMark(agent: agent)
            .frame(width: 13, height: 13)
    }
}

/// EXP-861: the queued-messages strip directly above the composer — one line
/// per message the agent has not read yet in a filled box (EXP-1213: the
/// task list under it wears the same one), each with a
/// trailing ghost X that revokes a HELD message
/// (`{"t":"unqueue","id"}`). EXP-873: a `sent` line (already with the agent,
/// awaiting its replay) draws no X — only Stop takes it back. The caller
/// hides the strip when the queue is empty or the run is over.
struct AgentQueueStrip: View {
    let messages: [QueuedMessage]
    /// N-12: inside THE Dock's sheet (no strip chrome, no column).
    var plain = false
    let onRemove: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(messages) { message in
                HStack(spacing: 8) {
                    AppIcon(AppIcons.uiQueued, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Text(message.text)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Spacer(minLength: 0)
                    if !message.sent {
                        Button {
                            onRemove(message.id)
                        } label: {
                            AppIcon(AppIcons.uiClose, size: 10, weight: .semibold)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .frame(width: 20, height: 20)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(AgentFeed.queueRemoveLabel)
                        .accessibilityIdentifier("agent-queue-remove")
                    }
                }
            }
        }
        .modifier(QueueStripChrome(plain: plain))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(AgentFeed.queueStripTitle)
        .accessibilityIdentifier("agent-queue-strip")
    }
}

/// The strip's own row and column; none inside THE Dock's sheet.
private struct QueueStripChrome: ViewModifier {
    let plain: Bool

    func body(content: Content) -> some View {
        if plain {
            content
        } else {
            content
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .glassRow(isOpaque: true)
                // EXP-927 §2c "Alignment": the same reading column as the
                // transcript above it.
                .transcriptColumn()
        }
    }
}

/// EXP-856 §4: a second copy of a live agent started (a `SendMessage` to a
/// running agent resumes it from its transcript, and that copy edits the same
/// files). An amber warning row carrying the wire's sentence VERBATIM — under
/// the workflow card when the edge named one, inline beside the subagent's row
/// otherwise, and kept when the card collapses.
struct AgentDuplicateWarningRow: View {
    let detail: String

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            AppIcon(AppIcons.uiWarning, size: 11)
                .foregroundStyle(DesignTokens.Semantic.yellow)
                .padding(.top, 2)
            Text(detail)
                .font(.caption2)
                .foregroundStyle(DesignTokens.Semantic.yellow)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("agent-duplicate-warning")
    }
}

/// EXP-850 §3 / EXP-1240: a claude workflow, rendered IN PLACE of its
/// `Workflow` tool row as THE Work card — the phases as a segment bar, the
/// finished ones folded into one row, the running phase's lanes one line
/// each. A lane opens that agent's own tab (EXP-1225: a workflow agent is a
/// conversation tab like any subagent). EXP-856: the duplicate warnings of
/// its agents sit under the card's own rows.
struct WorkflowWorkCardRow: View {
    let workflow: AgentWorkflow
    /// The run behind one agent id — the card reads its duplicate warning.
    let runFor: (String?) -> AgentSubagentRun?
    /// Open one agent's conversation tab.
    let onOpenAgent: (String) -> Void

    var body: some View {
        let card = workflow.workCard
        let folded = workflow.workCardFoldedPhases(card)
        let duplicates = workflow.agents.compactMap { runFor($0.agentId)?.duplicateDetail }
        WorkCardView(
            card: card,
            foldedPhases: folded.isEmpty ? nil : folded,
            onOpenLane: { phase, lane in
                if let agentId = workflow.workCardLaneAgentId(phase: phase, lane: lane) {
                    onOpenAgent(agentId)
                }
            }
        ) {
            DuplicateWarnings(details: duplicates)
        }
        .accessibilityIdentifier("agent-workflow-card")
    }
}

/// A subagent's work (protocol v2) as THE Work card (EXP-1240): its name, the
/// calls it made, done or running. The card opens the subagent's own tab,
/// where its rows live. EXP-748: the count is what the publisher reported
/// (`toolCount`), not the rows that survived the replay buffer.
struct SubagentWorkCardRow: View {
    let run: AgentSubagentRun
    let onOpen: () -> Void

    var body: some View {
        WorkCardView(card: AgentFeed.subagentWorkCard(run), onOpenSubagent: onOpen) {
            // EXP-856: a second copy of this agent started while the first
            // was still running — amber, verbatim, never folded away.
            DuplicateWarnings(details: run.duplicateDetail.map { [$0] } ?? [])
        }
        .accessibilityIdentifier("agent-subagent-card")
    }
}

/// EXP-856 §4: the duplicate warnings under a Work card's own rows.
private struct DuplicateWarnings: View {
    let details: [String]

    var body: some View {
        if !details.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(details, id: \.self) { detail in
                    AgentDuplicateWarningRow(detail: detail)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .overlay(alignment: .top) { GlassDivider() }
        }
    }
}

/// EXP-1279: a code-mode `exec` call as its Script card ("Ran a script · 59
/// calls · 3.0 s", the calls grouped by tool, the script behind "Script").
/// The model is ExpCore's (`ScriptCards.card`); an older codemode that logs
/// nothing is the bare "Ran a script".
struct ScriptCallRow: View {
    let settled: Bool
    let failed: Bool
    let script: AgentScriptLog?

    var body: some View {
        ScriptCardView(
            card: ScriptCards.card(ScriptCards.input(settled: settled, log: script)),
            running: !settled,
            failed: failed,
            source: script?.source
        )
    }
}

/// EXP-1241 (EXP-1259 D14): a SETTLED, successful `exponential_comments_create`
/// call renders the REAL comment — the synced row the result's comment ref
/// names (its body, author and time), under "Commented on <issue chip>".
/// Until the row syncs (or for a comment this phone cannot see) it is the
/// result's own preview: the ref's title as the body, the run's owner (whose
/// key posted it), "via MCP" and the settle's time. Tapping opens the issue
/// scrolled to the comment, which flashes (web `?comment=<id>`).
struct CommentCallRow: View {
    let detail: String?
    let preview: AgentToolPreview?
    let settledAt: Int?
    /// The run's coding agent, for the author row's mark.
    let agent: String?
    /// The run owner's name — the author until the row syncs.
    let ownerName: String?
    let context: AgentMarkdownContext

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var comment: CommentEntity?
    @State private var authorName: String?
    /// #VAPP-186: the app whose MCP tool wrote it ("via <app>").
    @State private var viaAppName: String?
    @State private var issueIdentifier: String?

    /// The contract `expToolDisplay` row of `exponential_comments_create`.
    static let tool = "comments_create"

    private var commentRef: EntityRef? { preview?.refs.first { $0.kind == "comment" } }
    private var issueRef: EntityRef? { preview?.refs.first { $0.kind == "issue" } }
    private var commentId: String? { commentRef?.id ?? preview?.id }

    /// The result's issue identifier, else the call's subject (its `issueId`
    /// input — an identifier or a uuid).
    private var issueKey: String? {
        if let issueIdentifier { return issueIdentifier }
        if let identifier = issueRef?.identifier { return identifier }
        let subject = detail?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return subject.isEmpty ? issueRef?.id : subject
    }

    @MainActor
    private var chip: IssueRefLookup.Chip? {
        guard let refs = context.issueRefs, let key = issueKey else { return nil }
        return IssueRefChipCache.chip(key, scope: .team(id: refs.teamId), db: refs.db, accountId: accountId)
    }

    private var bodyText: String? {
        let text = comment?.body ?? commentRef?.title ?? preview?.title
        let trimmed = text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? nil : text
    }

    private var caption: String {
        let source = ReporterReply.caption(
            source: comment?.source ?? DomainContract.commentSourceMcp,
            audience: comment?.audience ?? "team",
            authorAgentId: comment?.authorAgentId,
            viaAppName: viaAppName
        )
        let time: String? = if let comment {
            RelativeTime.long(wire: comment.createdAt)
        } else if let settledAt {
            RelativeTime.long(Date(timeIntervalSince1970: Double(settledAt) / 1000))
        } else {
            nil
        }
        return [source, time].compactMap { $0 }.joined(separator: " · ")
    }

    var body: some View {
        let chip = self.chip
        let issueId = comment?.issueId ?? chip?.issueId
        CommentCallCardView(
            author: comment == nil ? ownerName : authorName,
            caption: caption,
            running: false,
            hasBody: bodyText != nil,
            onOpen: issueId.flatMap { issueId in
                commentId.map { commentId in
                    {
                        deps.deepLinkBus.navigateToIssue(
                            issueId, accountId: accountId, commentId: commentId
                        )
                    }
                }
            }
        ) {
            if let key = issueKey {
                IssueChip(
                    identifier: key,
                    title: chip?.title,
                    iconName: chip?.status.iconName ?? AppIcons.uiIssue,
                    statusColor: chip?.status.color,
                    bodySize: DesignTokens.Transcript.bodySize,
                    onTap: chip.map { chip in { context.issueRefs?.onOpen(chip.issueId) } }
                )
            }
        } mark: {
            if let image = AgentBrandMark.image(agent ?? "claude") {
                image
                    .resizable()
                    .scaledToFit()
                    .frame(width: 14, height: 14)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        } body: {
            if let bodyText {
                AgentMarkdownText(text: bodyText, context: context)
            }
        }
        .task(id: commentId) { await observe() }
        .accessibilityIdentifier("comment-call-row")
    }

    /// The synced comment (it lands after the call settles), its author's
    /// name and its issue's identifier.
    private func observe() async {
        guard let commentId, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db -> (CommentEntity?, String?, String?, String?) in
            guard let row = try CommentEntity.fetchOne(db, key: commentId) else { return (nil, nil, nil, nil) }
            let author = try row.authorId.flatMap { try UserEntity.fetchOne(db, key: $0) }
            let issue = try IssueEntity.fetchOne(db, key: row.issueId)
            let app = try row.viaAppId.flatMap { try AppEntity.fetchOne(db, key: $0) }
            let appName = app?.deletedAt == nil ? app?.name : nil
            // #EXP-1383: an agent's comment names the AGENT.
            if let agent = try row.authorAgentId.flatMap({ try AgentDefinitionEntity.fetchOne(db, key: $0) }) {
                return (row, agent.name, issue?.identifier, appName)
            }
            return (row, memberDisplayName(author, id: row.authorId), issue?.identifier, appName)
        }
        do {
            for try await (row, author, identifier, appName) in observation.values(in: pool) {
                comment = row
                authorName = author
                viaAppName = appName
                if let identifier { issueIdentifier = identifier }
            }
        } catch {}
    }
}
