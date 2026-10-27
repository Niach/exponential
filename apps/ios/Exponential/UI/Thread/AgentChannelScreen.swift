import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// NEW-2/NEW-3: a team agent's page = the CALLER's DM with it (web
/// `t/$teamSlug/agents/$agentId`). The route names the agent DEFINITION
/// (`agents` row); the page shows the caller's own conversation session with
/// it — the root with that `agent_id` whose `user_id` is me — as THE thread
/// screen (`ThreadScreen`: the Conversation as a channel, the agent's
/// coloured icon + name in the header from the definition). No conversation
/// yet = the same screen, empty, whose composer opens one with
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

/// The DM before its first message: the header (the agent's coloured icon +
/// name), an empty Conversation, and THE floating composer (`Message
/// <name>`) whose send opens the conversation.
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
            AgentFirstMessageComposer(agent: agent)
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
                AgentCallButton(agentId: agent.id, agentName: agent.name)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-channel-page")
    }
}

/// THE floating composer (capsule → THE session composer box) for a DM that
/// has no session yet: text + files, sent as `sessions.send({agentId})`.
private struct AgentFirstMessageComposer: View {
    let agent: AgentDefinitionEntity

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.motion) private var motion
    @State private var draft: AgentFirstMessageDraft?
    @State private var attach = SessionComposerAttach()
    @State private var expanded = false

    private var placeholder: String { "Message \(agent.name)" }

    var body: some View {
        Group {
            if let draft {
                let open = expanded || draft.hasContent || attach.busy
                FloatingComposer(
                    placeholder: placeholder,
                    open: open,
                    collapsedLabel: placeholder,
                    collapsedIdentifier: "agent-composer-collapsed",
                    onExpand: expand
                ) {
                    SessionComposerCard(
                        draft: draft,
                        placeholder: placeholder,
                        controls: nil,
                        attach: $attach,
                        sendEnabled: !draft.steerSending,
                        onSend: { Task { await draft.sendDraft() } },
                        fieldIdentifier: "agent-composer-field"
                    )
                }
                .animation(motion.standard, value: open)
                .onChange(of: draft.sendError) { _, message in
                    guard let message else { return }
                    toaster.error(FeedbackStates.Errors.title(verb: "send", thing: "the message"), description: message)
                }
                .onChange(of: draft.draftEditor.isEditing) { _, editing in
                    guard expanded, !editing, !attach.busy, !draft.hasContent else { return }
                    withAnimation(motion.standard) { expanded = false }
                }
            } else {
                Color.clear.frame(height: 1)
            }
        }
        .frame(maxWidth: DesignTokens.Transcript.maxWidth)
        .frame(maxWidth: .infinity)
        .floatingBarEdge(!(expanded || draft?.hasContent == true))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-composer")
        .task(id: agent.id) {
            if draft?.agentId != agent.id {
                draft = AgentFirstMessageDraft(
                    agentId: agent.id,
                    teamId: agent.teamId,
                    accountId: accountId,
                    sessionsApi: deps.sessionsApi,
                    attachmentsApi: deps.attachmentsApi
                )
            }
        }
    }

    private func expand() {
        withAnimation(motion.standard) { expanded = true }
        DispatchQueue.main.async {
            guard let editor = draft?.draftEditor else { return }
            editor.setFocused(editor.blocks.first?.id)
        }
    }
}

/// The first message's draft: uploads go to the TEAM's session route (there
/// is no session yet), the send is `sessions.send({agentId, text,
/// attachmentIds})`, which opens the caller's conversation server-side.
@MainActor
@Observable
final class AgentFirstMessageDraft: SessionComposerDraft {
    let agentId: String
    let draftEditor = IssueEditorModel()
    var pendingImages: [PendingSteerImage] = []
    var pendingFiles: [PendingSteerFile] = []
    var steerImageError: String?
    var steerSending = false
    /// A refused send: the host toasts it (the draft stays put).
    var sendError: String?

    @ObservationIgnored private let teamId: String
    @ObservationIgnored private let accountId: String
    @ObservationIgnored private let sessionsApi: SessionsApi
    @ObservationIgnored private let attachmentsApi: AttachmentsApi

    init(agentId: String, teamId: String, accountId: String, sessionsApi: SessionsApi, attachmentsApi: AttachmentsApi) {
        self.agentId = agentId
        self.teamId = teamId
        self.accountId = accountId
        self.sessionsApi = sessionsApi
        self.attachmentsApi = attachmentsApi
    }

    func uploadAttachment(data: Data, filename: String, contentType: String) async throws -> UploadedAttachment {
        try await attachmentsApi.uploadTeamSessionImage(
            accountId: accountId,
            teamId: teamId,
            data: data,
            filename: filename,
            contentType: contentType
        )
    }

    func sendMessage(_ text: String, attachmentIds: [String]?) async -> Bool {
        sendError = nil
        let holdsFlag = !steerSending
        if holdsFlag { steerSending = true }
        defer { if holdsFlag { steerSending = false } }
        do {
            _ = try await sessionsApi.send(
                accountId: accountId,
                .agent(
                    agentId: agentId,
                    text: text.isEmpty ? nil : text,
                    attachmentIds: attachmentIds?.isEmpty == false ? attachmentIds : nil
                )
            )
            return true
        } catch {
            sendError = error.userFacingMessage
            return false
        }
    }
}
