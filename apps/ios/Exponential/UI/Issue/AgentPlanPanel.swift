import ExpCore
import ExpUI
import GRDB
import MarkdownUI
import SwiftUI

// Agent lifecycle events shown in the quiet activity feed (and used to detect a
// terminal error for the Retry affordance). Mirrors AGENT_EVENT_TYPES in
// apps/web/src/components/agent-plan-panel.tsx.
let agentEventTypes: Set<String> = [
    "agent_started", "plan_ready", "agent_question",
    "agent_answer", "pr_opened", "pr_merged", "agent_error",
]

/// First-class panel for the agent plan/question lifecycle, replacing the
/// plan/question comment rows (mirror of apps/web/src/components/agent-plan-panel.tsx).
/// State is driven by the synced `issue.agentPlanState`; the plan/question TEXT,
/// revision, and approval come from the synced `agent_runs` row.
struct AgentPlanPanel: View {
    let issue: IssueEntity
    let canApprovePlan: Bool

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var agentRun: AgentRunEntity?
    @State private var answer = ""
    @State private var busy: PanelAction?
    @State private var events: [IssueEventEntity] = []
    @State private var observationTask: Task<Void, Never>?
    @State private var runObservationTask: Task<Void, Never>?
    @State private var showDiff = false

    private enum PanelAction: Equatable { case approve, requestChanges, answer, retry }

    private var agentEvents: [IssueEventEntity] {
        events.filter { agentEventTypes.contains($0.type) }
    }
    private var latestIsError: Bool { agentEvents.last?.type == "agent_error" }
    private var finished: Bool { issue.status == "done" || issue.status == "cancelled" }
    private var implementing: Bool {
        !finished && issue.agentPlanState == "approved" && issue.prState == nil && !latestIsError
    }
    // Plan/question text come from the synced `agent_runs` row (server-authored
    // jsonb {text}); unwrap them the same way as a comment body.
    private var planText: String? {
        let t = getCommentBodyText(agentRun?.planText)
        return t.isEmpty ? nil : t
    }
    private var questionText: String? {
        let t = getCommentBodyText(agentRun?.question)
        return t.isEmpty ? nil : t
    }

    var body: some View {
        Group {
            if issue.agentPlanState != nil || latestIsError || issue.prUrl != nil {
                VStack(alignment: .leading, spacing: 10) {
                    header
                    content
                    if latestIsError { errorBanner }
                    prSection
                }
                .padding(12)
                .glassSection()
            }
        }
        .onAppear { startObserving() }
        .onDisappear {
            observationTask?.cancel()
            runObservationTask?.cancel()
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: 6) {
            Image(systemName: "sparkles").font(.caption).foregroundStyle(Color.accentColor)
            Text("Agent plan").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
            if let revision = agentRun?.planRevision, revision > 0 {
                Text("rev \(revision)")
                    .font(.caption2).foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            Spacer()
            if issue.agentPlanState == "approved", agentRun?.approvedAt != nil {
                HStack(spacing: 4) {
                    Image(systemName: "checkmark.circle.fill").font(.caption2)
                    Text("Approved").font(.caption2.weight(.medium))
                }
                .foregroundStyle(.green)
            }
        }
    }

    // MARK: - State content

    @ViewBuilder
    private var content: some View {
        switch issue.agentPlanState {
        case "drafting", "planning":
            loadingRow("Agent is working on a plan…")
        case "awaiting_answer":
            questionContent
        case "awaiting_approval", "approved":
            planContent
        default:
            EmptyView()
        }
    }

    private func loadingRow(_ text: String) -> some View {
        HStack(spacing: 8) {
            ProgressView().controlSize(.small).tint(.white)
            Text(text).font(.caption).foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }

    @ViewBuilder
    private var questionContent: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("The agent has a question", systemImage: "questionmark.circle")
                .font(.caption.weight(.medium)).foregroundStyle(.orange)
            if let q = questionText, !q.isEmpty {
                Markdown(q).markdownTheme(.gitHub).frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Text("Loading…").font(.caption).foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            if canApprovePlan {
                HStack(alignment: .bottom, spacing: 8) {
                    TextField("Answer the agent…", text: $answer, axis: .vertical)
                        .lineLimit(1...5)
                        .textFieldStyle(.plain)
                        .padding(.horizontal, 12).padding(.vertical, 8)
                        .background(Color.white.opacity(0.04))
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                        .foregroundStyle(.white)
                        .disabled(busy != nil)
                    Button {
                        let text = answer.trimmingCharacters(in: .whitespacesAndNewlines)
                        Task { await answerQuestion(text) }
                    } label: {
                        Group {
                            if busy == .answer {
                                ProgressView().controlSize(.mini).tint(.white)
                            } else {
                                Image(systemName: "paperplane.fill")
                            }
                        }
                        .padding(8)
                        .background(.blue, in: RoundedRectangle(cornerRadius: 8))
                        .foregroundStyle(.white)
                    }
                    .disabled(busy != nil || answer.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
        }
    }

    @ViewBuilder
    private var planContent: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let p = planText, !p.isEmpty {
                Markdown(p).markdownTheme(.gitHub).frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Text("Loading plan…").font(.caption).foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            if issue.agentPlanState == "awaiting_approval", canApprovePlan {
                HStack(spacing: 8) {
                    Button { Task { await runAction(.approve) { try await deps.agentPlanApi.approvePlan(accountId: accountId, issueId: issue.id) } } } label: {
                        actionLabel("Approve", systemImage: "checkmark", loading: busy == .approve)
                            .foregroundStyle(.green)
                            .background(.green.opacity(0.18), in: RoundedRectangle(cornerRadius: 6))
                    }
                    .buttonStyle(.plain)
                    .disabled(busy != nil)
                    Button { Task { await runAction(.requestChanges) { try await deps.agentPlanApi.requestChanges(accountId: accountId, issueId: issue.id) } } } label: {
                        actionLabel("Request changes", systemImage: "pencil", loading: busy == .requestChanges)
                            .glassButton()
                    }
                    .buttonStyle(.plain)
                    .disabled(busy != nil)
                }
            }
            if implementing {
                loadingRow("Agent is implementing the approved plan…")
            }
        }
    }

    private func actionLabel(_ title: String, systemImage: String, loading: Bool) -> some View {
        HStack(spacing: 6) {
            if loading { ProgressView().controlSize(.mini).tint(.white) }
            else { Image(systemName: systemImage) }
            Text(title)
        }
        .font(.caption.weight(.semibold))
        .padding(.horizontal, 12).padding(.vertical, 6)
    }

    // The PR (branch + link) and an inline diff disclosure, shown once the agent
    // has opened a pull request. Mirrors the macOS panel's PR section.
    @ViewBuilder
    private var prSection: some View {
        if let prUrl = issue.prUrl, let url = URL(string: prUrl) {
            VStack(alignment: .leading, spacing: 6) {
                if let branch = issue.branch, !branch.isEmpty {
                    Text(branch)
                        .font(.caption2.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                Link(destination: url) {
                    Label("View pull request", systemImage: "arrow.triangle.branch")
                        .font(.caption.weight(.medium))
                }
                .tint(Accent.indigo)
                DisclosureGroup("Changed files", isExpanded: $showDiff) {
                    DiffView(issueId: issue.id).padding(.top, 6)
                }
                .font(.caption)
                .tint(.white.opacity(TextOpacity.secondary))
            }
        }
    }

    @ViewBuilder
    private var errorBanner: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill").font(.caption).foregroundStyle(.red)
            Text("The agent hit an error.")
                .font(.caption).foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
            if canApprovePlan {
                Button { Task { await runAction(.retry) { try await deps.agentPlanApi.retry(accountId: accountId, issueId: issue.id) } } } label: {
                    HStack(spacing: 6) {
                        if busy == .retry { ProgressView().controlSize(.mini).tint(.white) }
                        else { Image(systemName: "arrow.clockwise") }
                        Text(busy == .retry ? "Retrying…" : "Retry")
                    }
                    .font(.caption.weight(.medium))
                    .padding(.horizontal, 10).padding(.vertical, 6)
                    .glassButton()
                }
                .buttonStyle(.plain)
                .disabled(busy != nil)
            }
        }
        .padding(10)
        .background(Color.red.opacity(0.08))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.red.opacity(0.25), lineWidth: 0.5))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    // MARK: - Actions

    private func runAction(_ action: PanelAction, _ fn: @escaping () async throws -> Void) async {
        busy = action
        defer { busy = nil }
        try? await fn()
    }

    private func answerQuestion(_ text: String) async {
        guard !text.isEmpty else { return }
        busy = .answer
        defer { busy = nil }
        do {
            try await deps.agentPlanApi.answerQuestion(accountId: accountId, issueId: issue.id, answer: text)
            answer = ""
        } catch {}
    }

    // MARK: - Event + agent-run observation

    // Both the activity feed and the plan/question text are driven entirely by
    // synced local rows now — no `agentPlan.getState` round-trip.
    private func startObserving() {
        observationTask?.cancel()
        runObservationTask?.cancel()
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        observationTask = Task {
            let eventObs = ValueObservation.tracking { db in
                try IssueEventEntity
                    .filter(Column("issue_id") == issue.id)
                    .order(Column("created_at").asc)
                    .fetchAll(db)
            }
            do {
                for try await rows in eventObs.values(in: pool) { self.events = rows }
            } catch {}
        }
        runObservationTask = Task {
            let runObs = ValueObservation.tracking { db in
                try AgentRunEntity.fetchOne(db, key: issue.id)
            }
            do {
                for try await row in runObs.values(in: pool) { self.agentRun = row }
            } catch {}
        }
    }
}

/// A quiet, collapsible feed of agent lifecycle events. Separate from the human
/// comment thread so routine agent activity doesn't read as conversation.
/// Mirror of apps/web/src/components/agent-activity-feed.tsx.
struct AgentActivityFeed: View {
    let events: [IssueEventEntity]
    let users: [String: UserEntity]
    @State private var expanded = false

    private var agentEvents: [IssueEventEntity] {
        events.filter { agentEventTypes.contains($0.type) }
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
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                if expanded {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(agentEvents) { event in
                            HStack(spacing: 8) {
                                Circle().fill(.white.opacity(TextOpacity.tertiary)).frame(width: 6, height: 6)
                                Text("\(who(event)) \(eventPhrase(event, users: users, labels: nil))")
                                    .font(.caption).foregroundStyle(.white.opacity(TextOpacity.secondary))
                                Spacer()
                            }
                            .padding(.vertical, 3)
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
        event.actorUserId.flatMap { users[$0] }.map { $0.name ?? $0.email } ?? "Agent"
    }
}

/// Human-readable verb for an issue/agent event type (the generic fallback used
/// when an event has no payload to render richly).
func agentEventVerb(_ type: String) -> String {
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

/// Human label for an issue_status enum value.
func statusLabel(_ s: String) -> String {
    switch s {
    case "backlog": return "Backlog"
    case "todo": return "Todo"
    case "in_progress": return "In Progress"
    case "done": return "Done"
    case "cancelled": return "Cancelled"
    default: return s.replacingOccurrences(of: "_", with: " ").capitalized
    }
}

/// Pull a string or integer scalar out of an issue_event's JSON payload (stored
/// as stringified JSON). Returns nil for missing/null/empty values.
func eventField(_ payload: String?, _ key: String) -> String? {
    guard let payload, let data = payload.data(using: .utf8),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let value = obj[key], !(value is NSNull) else { return nil }
    if let s = value as? String { return s.isEmpty ? nil : s }
    if let i = value as? Int { return String(i) }
    if let d = value as? Double { return String(Int(d)) }
    return nil
}

/// A rich activity phrase from the event type + payload (status from→to, PR #N,
/// assigned/unassigned, label name). Resolves user/label names when the maps are
/// supplied; falls back to the generic verb for events without a payload.
/// Mirrors the web activity timeline (and the Linux `eventPhrase`).
func eventPhrase(
    _ event: IssueEventEntity,
    users: [String: UserEntity],
    labels: [String: LabelEntity]?
) -> String {
    switch event.type {
    case "status_changed":
        guard let to = eventField(event.payload, "to") else { return "changed the status" }
        if let from = eventField(event.payload, "from") {
            return "changed status from \(statusLabel(from)) to \(statusLabel(to))"
        }
        return "changed status to \(statusLabel(to))"
    case "assignee_changed":
        guard let to = eventField(event.payload, "to") else { return "unassigned this issue" }
        if let name = users[to].map({ $0.name ?? $0.email }) {
            return "assigned \(name)"
        }
        return "assigned this issue"
    case "label_added":
        if let id = eventField(event.payload, "labelId"), let name = labels?[id]?.name {
            return "added label \(name)"
        }
        return "added a label"
    case "label_removed":
        if let id = eventField(event.payload, "labelId"), let name = labels?[id]?.name {
            return "removed label \(name)"
        }
        return "removed a label"
    case "pr_opened":
        if let n = eventField(event.payload, "prNumber") { return "opened PR #\(n)" }
        return "opened a pull request"
    case "pr_merged":
        if let n = eventField(event.payload, "prNumber") { return "merged PR #\(n)" }
        return "merged the pull request"
    default:
        return agentEventVerb(event.type)
    }
}
