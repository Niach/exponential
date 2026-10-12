import ExpCore
import ExpUI
import SwiftUI

/// EXP-1284: the thread's CONVERSATION child (brief §2), phones = draft 3:
/// the `conversation` blocks of THE thread view (`ThreadViews.build`) in
/// order — a `Run N` block per run (header row: the agent's run mark, the
/// label, its caption parts, a hairline, the state word), the issues its
/// agent linked as rows under it, and the runs one parent started as ONE
/// parallel card. Presentational: the screen (and the styleguide specimen)
/// hand it the rows; every tap goes back out through a closure.
///
/// A run block's body is the EXISTING run content for that run: the run's
/// slice of the Guide (`sessionThread` over `ThreadViews.runResults`, the
/// Summary as the reply) closed by the Run face's status row
/// (`RunStatusRow`, `WorkFaces.runRowCaption`); the agent's link rows sit
/// right under the header (draft 3). A LIVE run's content stays owner-only
/// (EXP-312): a teammate sees the status row alone. An ended run folds to ONE
/// line (its name) with `Show work`, which opens that thread in place. The
/// run's full transcript (and the steer composer) is its own Work screen —
/// the header row opens it.
struct ThreadConversationView: View {
    let rows: ThreadRows
    let view: ThreadViewModel
    /// The viewer (`deps.auth.userId`).
    let me: String?
    var now: Date = Date()
    /// Opens a run's own Work screen (header row, parallel card row).
    var onOpenRun: (String) -> Void = { _ in }
    /// Opens an issue's child tab (a link row's chip).
    var onOpenIssue: (String) -> Void = { _ in }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.motion) private var motion
    /// Folded runs the reader opened with Show work.
    @State private var opened: Set<String> = []

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            ForEach(Array(view.conversation.enumerated()), id: \.offset) { _, block in
                switch block {
                case let .run(run):
                    runBlock(run)
                case let .parallel(parallel):
                    parallelBlock(parallel)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-conversation")
    }

    // MARK: - Run block

    @ViewBuilder
    private func runBlock(_ block: ThreadRunBlock) -> some View {
        let session = rows.session(id: block.runId)
        VStack(alignment: .leading, spacing: 10) {
            ThreadBlockHeader(
                agent: session?.agent,
                markState: markState(session),
                ended: block.state == .ended,
                label: block.label,
                caption: block.caption,
                state: block.state,
                onOpen: { onOpenRun(block.runId) }
            )
            // The issues its agent linked sit right under the header (draft
            // 3), then the run's own content.
            ForEach(block.links, id: \.itemId) { link in
                ThreadLinkRowView(
                    link: link,
                    agent: session?.agent,
                    issue: rows.issue(id: link.issueId),
                    onOpen: { onOpenIssue(link.issueId) }
                )
            }
            if let session {
                runBody(block, session: session)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-run-block")
    }

    @ViewBuilder
    private func runBody(_ block: ThreadRunBlock, session: CodingSessionEntity) -> some View {
        if block.folded, !opened.contains(block.runId) {
            // ONE line: the run's name, then Show work.
            HStack(spacing: 6) {
                Text(rows.name(of: session))
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.tail)
                Text("·")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Button(ThreadViews.showWorkLabel) {
                    withAnimation(motion.standard) { _ = opened.insert(block.runId) }
                }
                .buttonStyle(.plain)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.primary))
                .fixedSize()
                .accessibilityIdentifier("thread-show-work")
                Spacer(minLength: 0)
            }
            .padding(.leading, SessionRowLead.markSize + SessionRowLead.gap)
        } else {
            let owner = session.userId == me
            // EXP-312: a LIVE run's content is its owner's; an ended run's
            // published Guide is the team's anyway.
            if owner || block.state == .ended {
                runThread(session)
                    .padding(.leading, SessionRowLead.markSize + SessionRowLead.gap)
            }
            // The status row closes the block, where the run is NOW.
            statusRow(session)
        }
    }

    /// The Run face's ONE status row, off the synced row alone (the relay's
    /// tool line is the run screen's).
    private func statusRow(_ session: CodingSessionEntity) -> some View {
        let ended = PastRuns.hasEnded(session)
        let display = CodingSessionDisplayState.of(session: session, prState: nil)
        let device = rows.device(for: session, now: now)
        let paused = !ended && device.isPaused(display, status: session.status)
        let state = WorkFaces.runRowState(
            paused: paused,
            ended: ended,
            awaitingInput: false,
            working: CodingSessionDisplayState.working(
                status: session.status, state: display, paused: paused, live: true
            ),
            display: display
        )
        return RunStatusRow(
            agent: session.agent,
            markState: runningRowMarkState(display, paused: paused, working: state == .working),
            ended: ended,
            rowState: state,
            caption: { now in
                WorkFaces.runRowCaption(
                    state: state,
                    device: device.displayLabel,
                    startedAt: session.startedAt,
                    endedAt: session.endedAt ?? session.updatedAt,
                    now: now
                )
            },
            toolLine: nil,
            showWork: false,
            // Show work = the run's full transcript, its own screen.
            onToggle: { onOpenRun(session.id) }
        )
        .padding(.horizontal, -AgentSessionLayout.gutter)
    }

    /// The run's slice of the thread's Guide, in publish order, the Summary
    /// as the reply.
    @ViewBuilder
    private func runThread(_ session: CodingSessionEntity) -> some View {
        let thread = sessionThread(runResults(session))
        if !thread.isEmpty {
            VStack(alignment: .leading, spacing: 12) {
                ForEach(Array(thread.items.enumerated()), id: \.offset) { _, item in
                    switch item {
                    case let .text(topic, text):
                        VStack(alignment: .leading, spacing: 4) {
                            Text(topic)
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            AgentMarkdownText(text: text, context: markdownContext(session), options: [.autolinkBareURLs])
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    case let .picture(entry):
                        SessionInlinePicture(entry: entry)
                    }
                }
                if let reply = thread.reply {
                    AgentMarkdownText(
                        text: reply,
                        context: markdownContext(session),
                        options: [.autolinkBareURLs, .hardLineBreaks]
                    )
                }
            }
        }
    }

    /// The thread's Guide (else the run's own row), windowed to this run.
    private func runResults(_ session: CodingSessionEntity) -> String? {
        let source = ThreadViews.guideResults(threadResults: rows.thread.results, runResults: session.results)
        let index = rows.sessions.firstIndex { $0.id == session.id }
        let next = index.flatMap { rows.sessions.indices.contains($0 + 1) ? rows.sessions[$0 + 1].startedAt : nil }
        return ThreadViews.runResults(source, startedAt: session.startedAt, nextStartedAt: next)
    }

    private func markdownContext(_ session: CodingSessionEntity) -> AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            issueRefs: AgentIssueRefContext(
                teamId: session.teamId,
                db: deps.db,
                onOpen: { issueId in
                    deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
                }
            )
        )
    }

    /// A run's mark: the working mark while its agent works, amber while it
    /// waits on a person, else the bare brand mark.
    private func markState(_ session: CodingSessionEntity?) -> CodingSessionDisplayState? {
        guard let session, !PastRuns.hasEnded(session) else { return nil }
        if session.needsInput { return .needsInput }
        return session.agentBusy ? .working : nil
    }

    // MARK: - Parallel block

    @ViewBuilder
    private func parallelBlock(_ block: ThreadParallelBlock) -> some View {
        let first = block.runs.first.flatMap { rows.session(id: $0.runId) }
        VStack(alignment: .leading, spacing: 10) {
            ThreadBlockHeader(
                agent: first?.agent,
                markState: block.runs.contains { rows.session(id: $0.runId)?.agentBusy == true }
                    ? .working
                    : (block.state == .needsInput ? .needsInput : nil),
                ended: block.state == .ended,
                label: block.label,
                caption: block.caption,
                state: block.state,
                onOpen: nil
            )
            VStack(spacing: 0) {
                ForEach(Array(block.runs.enumerated()), id: \.element.runId) { index, row in
                    if index > 0 {
                        Rectangle()
                            .fill(.white.opacity(GlassMenuTokens.dividerOpacity))
                            .frame(height: GlassTokens.hairline)
                    }
                    parallelRow(row)
                }
            }
            .glassRow()
            .padding(.leading, SessionRowLead.markSize + SessionRowLead.gap)
            ForEach(block.links, id: \.itemId) { link in
                ThreadLinkRowView(
                    link: link,
                    agent: first?.agent,
                    issue: rows.issue(id: link.issueId),
                    onOpen: { onOpenIssue(link.issueId) }
                )
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-parallel-block")
    }

    private func parallelRow(_ row: ThreadParallelRow) -> some View {
        let session = rows.session(id: row.runId)
        let issue = row.issueId.flatMap { rows.issue(id: $0) }
        let device = session.map { rows.device(for: $0, now: now).displayLabel } ?? (row.deviceLabel ?? "")
        let age = RelativeTime.compact(wire: row.startedAt, now: now)
        return Button {
            onOpenRun(row.runId)
        } label: {
            HStack(spacing: SessionRowLead.gap) {
                AgentRunMark(
                    agent: session?.agent,
                    state: markState(session),
                    badgeSize: SessionRowLead.badgeSize,
                    ended: row.state == .ended
                )
                .frame(width: SessionRowLead.markSize, height: SessionRowLead.markSize)
                Text(row.name)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .layoutPriority(1)
                if let issue {
                    IssueChip(
                        identifier: issue.identifier,
                        title: nil,
                        iconName: nil,
                        statusColor: nil,
                        size: .sm
                    )
                    // The identifier never truncates; the name gives way.
                    .fixedSize()
                }
                Spacer(minLength: 4)
                Text([device, age].filter { !$0.isEmpty }.joined(separator: " · "))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .fixedSize()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("thread-parallel-row")
    }
}

/// EXP-1284: a block's header row — the agent's run mark (its spinner while
/// the agent works), `Run N` / `Runs 3 and 4`, the caption parts joined
/// ` · `, a hairline to the edge and the state word (`live` | `needs input` |
/// `ended`). A run block's row opens that run.
struct ThreadBlockHeader: View {
    let agent: String?
    let markState: CodingSessionDisplayState?
    let ended: Bool
    let label: String
    let caption: [String]
    let state: ThreadRunState
    let onOpen: (() -> Void)?

    var body: some View {
        if let onOpen {
            Button(action: onOpen) { row }
                .buttonStyle(.plain)
                .accessibilityIdentifier("thread-block-header")
        } else {
            row
        }
    }

    private var row: some View {
        HStack(spacing: SessionRowLead.gap) {
            AgentRunMark(agent: agent, state: markState, badgeSize: SessionRowLead.badgeSize, ended: ended)
                .frame(width: SessionRowLead.markSize, height: SessionRowLead.markSize)
                .accessibilityHidden(true)
            Text(label)
                .font(.footnote.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.primary))
                .fixedSize()
            if !caption.isEmpty {
                Text(caption.joined(separator: " · "))
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            Rectangle()
                .fill(.white.opacity(GlassMenuTokens.dividerOpacity * 2))
                .frame(height: GlassTokens.hairline)
                .frame(maxWidth: .infinity)
            Text(ThreadViews.stateWord(state))
                .font(.footnote)
                .foregroundStyle(stateColor)
                .fixedSize()
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    private var stateColor: Color {
        switch state {
        case .needsInput: DesignTokens.Semantic.yellow
        case .live, .ended: .white.opacity(TextOpacity.tertiary)
        }
    }
}

/// EXP-1284: an issue the run's agent linked, under its block — `<agent
/// mark> Linked | Filed and linked [issue chip]`; the chip opens that issue's
/// child.
struct ThreadLinkRowView: View {
    let link: ThreadLinkRow
    let agent: String?
    /// The synced row, for the chip's status glyph (nil = the anchor default).
    let issue: IssueEntity?
    let onOpen: () -> Void

    var body: some View {
        HStack(spacing: SessionRowLead.gap) {
            AgentRunMark(agent: agent, state: nil, ended: true)
                .frame(width: SessionRowLead.markSize, height: SessionRowLead.markSize)
                .accessibilityHidden(true)
            Text(link.label)
                .font(.footnote)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .fixedSize()
            IssueChip(
                identifier: link.identifier,
                title: link.title,
                status: IssueStatus.from(issue?.status),
                onTap: onOpen
            )
            .layoutPriority(-1)
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-link-row")
    }
}
