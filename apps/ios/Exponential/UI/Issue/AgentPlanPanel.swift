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
/// State is driven by the synced `issue` columns; the plan/question TEXT is
/// fetched via `agentPlan.getState` (server-only, not in Electric).
struct AgentPlanPanel: View {
    let issue: IssueEntity
    let canApprovePlan: Bool

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var planText: String?
    @State private var questionText: String?
    @State private var answer = ""
    @State private var busy: PanelAction?
    @State private var events: [IssueEventEntity] = []
    @State private var observationTask: Task<Void, Never>?

    private enum PanelAction: Equatable { case approve, requestChanges, answer, retry }

    private var agentEvents: [IssueEventEntity] {
        events.filter { agentEventTypes.contains($0.type) }
    }
    private var latestIsError: Bool { agentEvents.last?.type == "agent_error" }
    private var finished: Bool { issue.status == "done" || issue.status == "cancelled" }
    private var implementing: Bool {
        !finished && issue.agentPlanState == "approved" && issue.prState == nil && !latestIsError
    }
    // Re-fetch plan/question text whenever the synced state or revision moves.
    private var fetchKey: String { "\(issue.agentPlanState ?? "none")-\(issue.agentPlanRevision)" }

    var body: some View {
        Group {
            if issue.agentPlanState != nil || latestIsError {
                VStack(alignment: .leading, spacing: 10) {
                    header
                    content
                    if latestIsError { errorBanner }
                }
                .padding(12)
                .glassSection()
            }
        }
        .task(id: fetchKey) { await loadPlanState() }
        .onAppear { startObserving() }
        .onDisappear { observationTask?.cancel() }
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: 6) {
            Image(systemName: "sparkles").font(.caption).foregroundStyle(Color.accentColor)
            Text("Agent plan").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
            if issue.agentPlanRevision > 0 {
                Text("rev \(issue.agentPlanRevision)")
                    .font(.caption2).foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            Spacer()
            if issue.agentPlanState == "approved", issue.agentPlanApprovedAt != nil {
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

    // MARK: - Plan fetch + event observation

    private func loadPlanState() async {
        let state = issue.agentPlanState
        guard state == "awaiting_approval" || state == "awaiting_answer" || state == "approved" else {
            planText = nil
            questionText = nil
            return
        }
        do {
            let r = try await deps.agentPlanApi.getState(accountId: accountId, issueId: issue.id)
            planText = r.planText
            questionText = r.question
        } catch {
            // Leave existing values; the UI falls back to "Loading…".
        }
    }

    private func startObserving() {
        observationTask?.cancel()
        observationTask = Task {
            guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
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
                                Text("\(who(event)) \(agentEventVerb(event.type))")
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

/// Human-readable verb for an issue/agent event type.
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
