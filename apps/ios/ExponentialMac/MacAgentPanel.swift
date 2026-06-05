import ExpCore
import ExpUI
import SwiftUI

// Agent lifecycle events shown in the quiet activity feed (and used to detect a
// terminal error for the Retry affordance). Mirrors AGENT_EVENT_TYPES in
// apps/web/src/components/agent-plan-panel.tsx.
let macAgentEventTypes: Set<String> = [
    "agent_started", "plan_ready", "agent_question",
    "agent_answer", "pr_opened", "pr_merged", "agent_error",
]

/// The first-class agent panel on the issue detail (web/Linux parity). Drives the
/// whole plan/question lifecycle off the synced `issue.agentPlanState`, fetching
/// the plan/question TEXT via `agentPlan.getState` (server-only, not in Electric):
///
/// - `drafting`/`planning` → "working on a plan…"
/// - `awaiting_approval`   → plan markdown + Approve / Request changes (+ Approve
///   & continue here on a registered desktop)
/// - `awaiting_answer`     → the question + an inline answer box
/// - `approved`            → plan + "Approved" + "implementing…"
/// - latest `agent_error`  → error banner + Retry
///
/// Also keeps the PR "Changes" button, branch name, cancel-while-running, and the
/// inline diff. Renders nothing when the issue has no agent activity.
struct MacAgentPanel: View {
    @Environment(MacAppDependencies.self) private var deps
    let model: MacIssueDetailModel
    let issue: IssueEntity

    @State private var planText: String?
    @State private var questionText: String?
    @State private var answerDraft = ""
    @State private var busy: AgentAction?
    @State private var showDiff = false

    private enum AgentAction: Equatable { case approve, requestChanges, approveContinue, answer, retry }

    // Plan states where a run is actively executing (so "Cancel" makes sense).
    private static let busyStates: Set<String> = ["drafting", "planning", "coding", "approved"]

    private var agentEvents: [IssueEventEntity] {
        model.issueEvents.filter { macAgentEventTypes.contains($0.type) }
    }
    private var latestIsError: Bool { agentEvents.last?.type == "agent_error" }

    private var canApprove: Bool {
        model.permissions?.canApprovePlan(creatorId: issue.creatorId) ?? false
    }
    private var canRunInteractive: Bool {
        guard let wid = model.workspaceId else { return false }
        return deps.agentService.canRunInteractive(workspaceId: wid)
    }
    private var finished: Bool { issue.status == "done" || issue.status == "cancelled" }
    private var implementing: Bool {
        !finished && issue.agentPlanState == "approved" && issue.prState == nil && !latestIsError
    }

    // Re-fetch the plan/question text whenever the synced state or revision moves.
    private var fetchKey: String { "\(issue.agentPlanState ?? "none")-\(issue.agentPlanRevision)" }

    var body: some View {
        if issue.agentPlanState != nil || issue.prUrl != nil || latestIsError {
            VStack(alignment: .leading, spacing: 10) {
                header
                if let branch = issue.branch, !branch.isEmpty {
                    Text(branch).font(.caption.monospaced()).foregroundStyle(.secondary)
                }
                stateContent
                if latestIsError { errorBanner }
                if issue.prUrl != nil {
                    DisclosureGroup("Changed files", isExpanded: $showDiff) {
                        MacDiffView(accountId: model.accountId, issueId: issue.id)
                    }
                    .font(.subheadline)
                }
            }
            .padding(12)
            .glassSection()
            .task(id: fetchKey) { await loadPlanState() }
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: 8) {
            Image(systemName: "cpu").foregroundStyle(Accent.indigo)
            Text("Agent").font(.subheadline.weight(.semibold))
            statusChip
            if issue.agentPlanState == "approved", issue.agentPlanApprovedAt != nil {
                approvedBadge
            }
            Spacer()
            if let wid = model.workspaceId, canRunInteractive,
               let state = issue.agentPlanState, Self.busyStates.contains(state) {
                Button(role: .destructive) {
                    deps.agentService.cancelIssue(workspaceId: wid, issueId: issue.id)
                } label: {
                    Label("Cancel", systemImage: "stop.circle")
                }
                .controlSize(.small)
                .help("Cancel the agent run for this issue")
            }
            if let prUrl = issue.prUrl, let url = URL(string: prUrl) {
                Button { Platform.open(url) } label: {
                    Label("Changes", systemImage: "arrow.triangle.branch")
                }
                .controlSize(.small)
                .help("View the pull request")
            }
        }
    }

    private var statusChip: some View {
        let s = agentStatus(issue)
        return Text(s.0)
            .font(.caption.weight(.medium))
            .padding(.horizontal, 8).padding(.vertical, 2)
            .background(s.1.opacity(0.15))
            .foregroundStyle(s.1)
            .clipShape(Capsule())
    }

    private var approvedBadge: some View {
        Label("Approved", systemImage: "checkmark")
            .font(.caption2.weight(.medium))
            .padding(.horizontal, 6).padding(.vertical, 2)
            .background(Color.green.opacity(0.15))
            .foregroundStyle(.green)
            .clipShape(Capsule())
    }

    private func agentStatus(_ issue: IssueEntity) -> (String, Color) {
        if issue.prState == "merged" { return ("Merged", .purple) }
        if issue.prState == "open" { return ("In review", .green) }
        if latestIsError { return ("Error", .red) }
        switch issue.agentPlanState {
        case "drafting", "planning": return ("Planning…", .blue)
        case "awaiting_approval": return ("Plan ready", Accent.indigo)
        case "awaiting_answer": return ("Needs answer", .orange)
        case "approved", "coding": return ("Coding…", .blue)
        default: return ("Agent", Color.secondary)
        }
    }

    // MARK: - State content

    @ViewBuilder
    private var stateContent: some View {
        switch issue.agentPlanState {
        case "drafting", "planning":
            HStack(spacing: 6) {
                ProgressView().controlSize(.small)
                Text("Agent is working on a plan…").font(.caption).foregroundStyle(.secondary)
            }
        case "awaiting_answer":
            questionContent
        case "awaiting_approval", "approved":
            planContent
        default:
            EmptyView()
        }
    }

    @ViewBuilder
    private var questionContent: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("The agent has a question", systemImage: "questionmark.circle")
                .font(.caption.weight(.medium)).foregroundStyle(.orange)
            if let q = questionText, !q.isEmpty {
                Text(q).font(.callout).textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Text("Loading…").font(.caption).foregroundStyle(.secondary)
            }
            if canApprove {
                HStack(alignment: .bottom, spacing: 8) {
                    TextField("Answer the agent…", text: $answerDraft, axis: .vertical)
                        .textFieldStyle(.roundedBorder)
                        .lineLimit(1...5)
                        .disabled(busy != nil)
                    Button {
                        let answer = answerDraft.trimmingCharacters(in: .whitespacesAndNewlines)
                        Task { @MainActor in
                            busy = .answer
                            await model.answerQuestion(answer)
                            answerDraft = ""
                            busy = nil
                        }
                    } label: {
                        if busy == .answer { ProgressView().controlSize(.small) }
                        else { Image(systemName: "paperplane.fill") }
                    }
                    .disabled(busy != nil || answerDraft.trimmingCharacters(in: .whitespaces).isEmpty)
                    .help("Send your answer")
                }
            }
        }
    }

    @ViewBuilder
    private var planContent: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let p = planText, !p.isEmpty {
                Text(p).font(.callout).textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Text("Loading plan…").font(.caption).foregroundStyle(.secondary)
            }
            if issue.agentPlanState == "awaiting_approval", canApprove {
                HStack(spacing: 8) {
                    Button {
                        Task { @MainActor in
                            busy = .approve
                            await model.approvePlan()
                            busy = nil
                        }
                    } label: {
                        Label("Approve", systemImage: "checkmark")
                    }
                    .disabled(busy != nil)
                    Button {
                        Task { @MainActor in
                            busy = .requestChanges
                            await model.requestChanges()
                            busy = nil
                        }
                    } label: {
                        Label("Request changes", systemImage: "pencil")
                    }
                    .disabled(busy != nil)
                    if canRunInteractive, let wid = model.workspaceId {
                        Button {
                            Task { @MainActor in
                                busy = .approveContinue
                                await model.approveAndContinue(workspaceId: wid)
                                busy = nil
                            }
                        } label: {
                            Label("Approve & continue here", systemImage: "play.circle")
                        }
                        .disabled(busy != nil)
                        .help("Approve and resume the interactive session on this Mac")
                    }
                }
                .controlSize(.small)
            }
            if implementing {
                HStack(spacing: 6) {
                    ProgressView().controlSize(.small)
                    Text("Agent is implementing the approved plan…")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }

    @ViewBuilder
    private var errorBanner: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.red)
            Text("The agent hit an error.").font(.caption).foregroundStyle(.secondary)
            Spacer()
            if canApprove {
                Button {
                    Task { @MainActor in
                        busy = .retry
                        await model.retry()
                        busy = nil
                    }
                } label: {
                    if busy == .retry { Text("Retrying…") } else { Text("Retry") }
                }
                .controlSize(.small)
                .disabled(busy != nil)
            }
        }
        .padding(8)
        .background(Color.red.opacity(0.08))
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    // MARK: - Plan fetch

    private func loadPlanState() async {
        let state = issue.agentPlanState
        guard state == "awaiting_approval" || state == "awaiting_answer" || state == "approved" else {
            planText = nil
            questionText = nil
            return
        }
        do {
            let r = try await deps.agentPlanApi.getState(accountId: model.accountId, issueId: issue.id)
            planText = r.planText
            questionText = r.question
        } catch {
            // Leave the existing values; the UI falls back to "Loading…".
        }
    }
}

/// A quiet, collapsible feed of agent lifecycle events (started, plan ready,
/// question, answer, PR opened/merged, error). Separate from the human comment
/// thread so routine agent activity doesn't read as conversation. Mirrors
/// apps/web/src/components/agent-activity-feed.tsx.
struct MacAgentActivityFeed: View {
    let events: [IssueEventEntity]
    let user: (String?) -> UserEntity?
    @State private var expanded = false

    private var agentEvents: [IssueEventEntity] {
        events.filter { macAgentEventTypes.contains($0.type) }
            .sorted { $0.createdAt < $1.createdAt }
    }

    var body: some View {
        if !agentEvents.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                Button { expanded.toggle() } label: {
                    HStack(spacing: 6) {
                        Image(systemName: expanded ? "chevron.down" : "chevron.right").font(.caption2)
                        Image(systemName: "bolt.horizontal").font(.caption2)
                        Text("Agent activity (\(agentEvents.count))").font(.caption)
                        Spacer()
                    }
                    .foregroundStyle(.secondary)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                if expanded {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(agentEvents) { event in
                            HStack(spacing: 8) {
                                Circle().fill(Color.secondary.opacity(0.5)).frame(width: 6, height: 6)
                                Text("\(who(event)) \(macAgentEventVerb(event.type))")
                                    .font(.caption).foregroundStyle(.secondary)
                                Text(macRelativeDate(event.createdAt)).font(.caption2).foregroundStyle(.tertiary)
                                Spacer()
                            }
                            .padding(.vertical, 2)
                        }
                    }
                    .padding(.top, 6)
                }
            }
            .padding(10)
            .background(Color.white.opacity(0.03))
            .clipShape(RoundedRectangle(cornerRadius: 8))
        }
    }

    private func who(_ event: IssueEventEntity) -> String {
        user(event.actorUserId).map { $0.name ?? $0.email } ?? "Agent"
    }
}

/// Human-readable verb for an agent/issue event type. Shared by the activity
/// feed and any inline event rows.
func macAgentEventVerb(_ type: String) -> String {
    switch type {
    case "status_changed": return "changed the status"
    case "assignee_changed": return "changed the assignee"
    case "label_added": return "added a label"
    case "label_removed": return "removed a label"
    case "pr_opened": return "opened a pull request"
    case "pr_merged": return "merged the pull request"
    case "plan_ready": return "posted a plan for review"
    case "agent_error": return "hit an error"
    case "agent_started": return "started working"
    case "agent_question": return "asked a question"
    case "agent_answer": return "answered the agent"
    default: return type.replacingOccurrences(of: "_", with: " ")
    }
}

/// The PR diff for an issue, loaded once from `issues.prFiles`. Renders each
/// changed file as a collapsible patch block with +/−/context line coloring.
struct MacDiffView: View {
    @Environment(MacAppDependencies.self) private var deps
    let accountId: String
    let issueId: String
    @State private var files: [PrFile]?
    @State private var loadError: String?
    @State private var loaded = false

    var body: some View {
        Group {
            if let loadError {
                Text(loadError).font(.caption).foregroundStyle(.secondary)
            } else if let files {
                if files.isEmpty {
                    Text("No changes yet.").font(.caption).foregroundStyle(.secondary)
                } else {
                    VStack(alignment: .leading, spacing: 8) {
                        ForEach(files) { fileBlock($0) }
                    }
                }
            } else {
                ProgressView().controlSize(.small)
            }
        }
        .task { await load() }
    }

    private func load() async {
        guard !loaded else { return }
        loaded = true
        do {
            files = try await deps.issuesApi.prFiles(accountId: accountId, issueId: issueId).files
        } catch {
            loadError = "Couldn't load changes from GitHub."
        }
    }

    @ViewBuilder
    private func fileBlock(_ file: PrFile) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 8) {
                Text(file.filename).font(.caption.monospaced()).lineLimit(1).truncationMode(.middle)
                Spacer()
                Text("+\(file.additions)").font(.caption2).foregroundStyle(.green)
                Text("−\(file.deletions)").font(.caption2).foregroundStyle(.red)
            }
            if let patch = file.patch, !patch.isEmpty {
                patchView(patch)
            }
        }
        .padding(8)
        .background(Color.white.opacity(0.03))
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    @ViewBuilder
    private func patchView(_ patch: String) -> some View {
        let lines = patch.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                Text(line.isEmpty ? " " : line)
                    .font(.system(.caption2, design: .monospaced))
                    .foregroundStyle(lineColor(line))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(lineBackground(line))
            }
        }
        .textSelection(.enabled)
    }

    private func lineColor(_ line: String) -> Color {
        if line.hasPrefix("@@") { return Accent.indigo }
        if line.hasPrefix("+") { return .green }
        if line.hasPrefix("-") { return .red }
        return Color.secondary
    }

    private func lineBackground(_ line: String) -> Color {
        if line.hasPrefix("@@") { return .clear }
        if line.hasPrefix("+") { return Color.green.opacity(0.08) }
        if line.hasPrefix("-") { return Color.red.opacity(0.08) }
        return .clear
    }
}
