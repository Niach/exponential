import ExpCore
import ExpUI
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
/// brand mark rather than the generic assistant sparkle. The mark is what
/// pulses; the text stays put, so a caption that ticks every second is still
/// readable. Static under Reduce Motion.
struct WorkingIndicatorRow: View {
    /// The run's coding agent (contract `codingAgent`), for the brand mark.
    /// Nil (or an id outside the contract) draws the neutral agents glyph.
    var agent: String?
    /// §5's caption at `now`, re-derived by the caller on every tick.
    let caption: (Date) -> String

    @Environment(\.motion) private var motion
    @State private var pulsing = false

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

    @ViewBuilder
    private var mark: some View {
        // EXP-849: never a bare `Image("agent-…")` — the brand marks are
        // hand-maintained assets and an unknown id falls back to the shared
        // `settings-agents` concept.
        if let agent, let image = AgentBrandMark.image(agent) {
            image
                .resizable()
                .scaledToFit()
                .frame(width: 13, height: 13)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .opacity(pulsing ? 1 : 0.4)
                .onAppear { startPulse() }
        } else {
            AppIcon(AppIcons.settingsAgents, size: 11)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .opacity(pulsing ? 1 : 0.4)
                .onAppear { startPulse() }
        }
    }

    /// §5: opacity 0.4 ↔ 1 over 1.4s, ease-in-out, autoreversing. The flag
    /// must STAY false under Reduce Motion — it drives the resting opacity
    /// too, so flipping it with a nil animation would pin the mark at 0.4.
    private func startPulse() {
        guard !motion.reduceMotion else {
            pulsing = true
            return
        }
        withAnimation(.easeInOut(duration: 1.4).repeatForever(autoreverses: true)) {
            pulsing = true
        }
    }
}

/// EXP-850 §1/§2: the compact strip directly above the composer — one line per
/// background task (`↻ {description}`) and one per OPEN wait row (`Waiting on
/// {detail}`). Absent when both are empty; the wait row itself stays an
/// ordinary tool row in the transcript.
struct AgentBottomStrip: View {
    let lines: [AgentStripLine]

    var body: some View {
        if !lines.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(lines) { line in
                    HStack(spacing: 8) {
                        AppIcon(glyph(line), size: 11)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        Text(line.text)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(1)
                            .truncationMode(.tail)
                        Spacer(minLength: 0)
                    }
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .glassRow()
            .padding(.horizontal, 14)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("agent-background-strip")
        }
    }

    private func glyph(_ line: AgentStripLine) -> String {
        switch line.kind {
        case .backgroundTask: AppIcons.uiRefresh
        case .wait: AppIcons.uiClock
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

/// EXP-850 §3: a claude workflow, rendered IN PLACE of its `Workflow` tool row
/// — the card the latest-wins `workflow` event paints: name and description, a
/// phase strip with per-phase queued/running/done/error counts, one nested row
/// per agent, and the summary once it finishes.
///
/// A workflow's agents are never subagent tabs and are never steerable; an
/// agent row expands to a collapsible preview of that agent's nested events
/// when it has any.
struct AgentWorkflowCardRow: View {
    let workflow: AgentWorkflow
    /// The nested run behind one agent id — the card's expandable preview and
    /// its duplicate warning come out of it.
    let runFor: (String?) -> AgentSubagentRun?
    let context: AgentMarkdownContext

    @State private var expandedAgents: Set<Int> = []

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            header
            if let description = workflow.description, !description.isEmpty {
                Text(description)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize(horizontal: false, vertical: true)
            }
            phaseStrip
            VStack(alignment: .leading, spacing: 4) {
                ForEach(workflow.agents) { agent in
                    agentRow(agent)
                }
            }
            if let summary = workflow.summary, !summary.isEmpty {
                Text(summary)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-workflow-card")
    }

    private var header: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.relationParent, size: 11)
                .foregroundStyle(DesignTokens.Semantic.blue)
            Text(workflow.name)
                .transcriptToolText(.medium)
                .foregroundStyle(.white)
                .lineLimit(1)
            Text(statusWord)
                .font(.caption2)
                .foregroundStyle(statusColor)
            Spacer(minLength: 0)
        }
    }

    private var statusWord: String {
        switch workflow.status {
        case .running: "running…"
        case .completed: "done"
        case .failed: "failed"
        case .stopped: "stopped"
        }
    }

    private var statusColor: Color {
        switch workflow.status {
        case .running: .white.opacity(TextOpacity.tertiary)
        case .completed: DesignTokens.Semantic.green
        case .failed: DesignTokens.Semantic.red
        case .stopped: .white.opacity(TextOpacity.tertiary)
        }
    }

    /// The phase strip: one chip per phase with its agents' state counts.
    @ViewBuilder
    private var phaseStrip: some View {
        if !workflow.phases.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(workflow.phases) { phase in
                        phaseChip(phase)
                    }
                }
            }
        }
    }

    private func phaseChip(_ phase: AgentWorkflowPhase) -> some View {
        let counts = workflow.counts(inPhase: phase.index)
        return HStack(spacing: 6) {
            Text(phase.title.isEmpty ? "Phase \(phase.index)" : phase.title)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            if counts.total > 0 {
                Text(AgentFeed.workflowPhaseCaption(counts))
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(GlassTokens.fillActive, in: RoundedRectangle(cornerRadius: DesignTokens.Radius.sm))
    }

    @ViewBuilder
    private func agentRow(_ agent: AgentWorkflowAgent) -> some View {
        let run = runFor(agent.agentId)
        let nested = run?.items ?? []
        let expanded = expandedAgents.contains(agent.index)
        VStack(alignment: .leading, spacing: 2) {
            Button {
                guard !nested.isEmpty else { return }
                if expanded {
                    expandedAgents.remove(agent.index)
                } else {
                    expandedAgents.insert(agent.index)
                }
            } label: {
                agentHeader(agent, expandable: !nested.isEmpty, expanded: expanded)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(nested.isEmpty)
            if let detail = agentDetail(agent) {
                Text(detail)
                    .font(.caption2)
                    .foregroundStyle(detailColor(agent))
                    .lineLimit(2)
                    .padding(.leading, 19)
            }
            // EXP-856: the warning stays whether or not the row is expanded.
            if let duplicate = run?.duplicateDetail {
                AgentDuplicateWarningRow(detail: duplicate)
                    .padding(.leading, 19)
            }
            if expanded, !nested.isEmpty {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(nested) { item in
                        SubagentItemRow(item: item, context: context)
                    }
                }
                .padding(.leading, 19)
            }
        }
    }

    private func agentHeader(
        _ agent: AgentWorkflowAgent, expandable: Bool, expanded: Bool
    ) -> some View {
        HStack(spacing: 6) {
            AppIcon(Self.stateGlyph(agent.state), size: 11)
                .foregroundStyle(Self.stateColor(agent.state))
            Text(agent.label)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
            if let model = agent.model, !model.isEmpty {
                Text(model)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if let telemetry = AgentFeed.workflowAgentTelemetry(agent) {
                Text(telemetry)
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }
            if expandable {
                AppIcon(expanded ? AppIcons.uiChevronDown : AppIcons.uiChevronRight, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
    }

    /// The agent's own second line: its error when it failed, its result once
    /// done, and what it is doing while it runs.
    private func agentDetail(_ agent: AgentWorkflowAgent) -> String? {
        switch agent.state {
        case .error: agent.error ?? agent.resultPreview
        case .done: agent.resultPreview
        case .running: agent.lastToolSummary ?? agent.lastTool
        case .queued: nil
        }
    }

    private func detailColor(_ agent: AgentWorkflowAgent) -> Color {
        agent.state == .error
            ? DesignTokens.Semantic.red
            : .white.opacity(TextOpacity.tertiary)
    }

    static func stateGlyph(_ state: AgentWorkflowAgentState) -> String {
        switch state {
        case .queued: AppIcons.statusBacklog
        case .running: AppIcons.codingRunning
        case .done: AppIcons.uiSuccess
        case .error: AppIcons.uiError
        }
    }

    static func stateColor(_ state: AgentWorkflowAgentState) -> Color {
        switch state {
        case .queued: .white.opacity(TextOpacity.quaternary)
        case .running: DesignTokens.Semantic.blue
        case .done: DesignTokens.Semantic.green
        case .error: DesignTokens.Semantic.red
        }
    }
}
