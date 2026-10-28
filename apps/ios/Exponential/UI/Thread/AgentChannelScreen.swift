import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// NEW-2/NEW-3: a team agent's page = the CALLER's conversation with it
/// (web `t/$teamSlug/agents/$agentId`). The route names the agent DEFINITION
/// (`agents` row); the page shows the caller's own conversation session with
/// it — the root with that `agent_id` whose `user_id` is me — as THE thread
/// screen (`ThreadScreen`, S-3 chat mode `bubbles`). A THIN wrapper stays
/// only for an agent WITHOUT a conversation yet: the empty screen with THE
/// thread composer (`ThreadComposer`, `agentId`) whose first send is
/// `sessions.send({agentId, text, attachmentIds})`; the page switches to the
/// conversation the moment its row syncs.
struct AgentChannelScreen: View {
    let agentId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var model = AgentChannelModel()

    var body: some View {
        Group {
            if let conversationId = model.conversationId {
                ThreadScreen(rootId: conversationId)
                    .id(conversationId)
            } else if let agent = model.agent {
                AgentChannelEmpty(agent: agent)
            } else if model.missing {
                ZStack {
                    AppBackground()
                    EmptyState(.threadNotFound)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .accessibilityIdentifier("agent-channel-not-found")
                }
            } else {
                ZStack {
                    AppBackground()
                    // EXP-1275 (D8): a first load is skeleton rows.
                    SkeletonRows()
                        .padding(.horizontal, 16)
                        .padding(.top, 8)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                }
            }
        }
        .onAppear {
            model.bind(accountId: accountId, agentId: agentId, userId: deps.auth.userId, db: deps.db)
        }
        .onDisappear { model.stop() }
    }
}

/// The agent's definition row and the caller's conversation with it, live
/// off GRDB. A definition that never arrives turns `missing` after a grace
/// period (web `SYNC_GRACE_MS`).
@MainActor
@Observable
final class AgentChannelModel {
    private(set) var agent: AgentDefinitionEntity?
    private(set) var conversationId: String?
    private(set) var missing = false

    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private var graceTask: Task<Void, Never>?

    nonisolated init() {}

    func bind(accountId: String, agentId: String, userId: String?, db: DatabaseManager) {
        stop()
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db -> (AgentDefinitionEntity?, String?) in
            let agent = try AgentDefinitionEntity.fetchOne(db, key: agentId)
            let conversation = try SessionEntity
                .filter(Column("agent_id") == agentId)
                .filter(Column("user_id") == (userId ?? ""))
                .filter(Column("parent_id") == nil)
                .order(Column("created_at"), Column("id"))
                .fetchOne(db)
            return (agent, conversation?.id)
        }
        task = Task { [weak self] in
            do {
                for try await (agent, conversationId) in observation.values(in: pool) {
                    guard let self else { return }
                    self.agent = agent
                    self.conversationId = conversationId
                    if agent != nil || conversationId != nil { self.missing = false }
                }
            } catch {}
        }
        graceTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(4))
            guard !Task.isCancelled, let self, self.agent == nil, self.conversationId == nil else { return }
            self.missing = true
        }
    }

    func stop() {
        task?.cancel()
        task = nil
        graceTask?.cancel()
        graceTask = nil
    }
}

/// The conversation before its first message: the header (the agent's
/// coloured icon + name), an empty Conversation, and THE thread composer
/// (`Message <name>`) whose first send opens the conversation.
private struct AgentChannelEmpty: View {
    let agent: AgentDefinitionEntity

    var body: some View {
        ZStack {
            AppBackground()
            Color.clear
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .contentShape(Rectangle())
                .onTapGesture { UIApplication.endEditing() }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            ThreadComposer(
                sessionId: "",
                placeholder: "Message \(agent.name)",
                agentId: agent.id,
                teamId: agent.teamId
            )
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                ThreadHeaderTitle(
                    title: agent.name,
                    agentIcon: BoardTypeDisplay.iconName(for: agent.icon) ?? SessionTree.ListCopy.agentDefaultIcon,
                    agentColor: agent.color ?? SessionTree.ListCopy.agentDefaultColor
                )
            }
            // NEW-69 (voice-call.json): the telephone (no `…` before the
            // first message).
            ToolbarItem(placement: .topBarTrailing) {
                AgentCallButton(
                    agentId: agent.id, agentName: agent.name,
                    agentIcon: BoardTypeDisplay.iconName(for: agent.icon), agentColor: agent.color
                )
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-channel-page")
    }
}
