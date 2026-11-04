import Foundation

/// A pending attachment of the extension's composer.
struct ShareAttachment: Identifiable, Sendable {
    let id = UUID()
    let data: Data
    let filename: String
    let contentType: String
    let isImage: Bool
}

/// Where a chat send goes.
enum ShareChatDestination: Equatable, Sendable {
    /// The viewer's conversation with an agent (`conversation` nil = none
    /// yet, the send creates it).
    case agent(id: String, conversation: MirroredSealRoot?)
    /// An existing chat (direct share).
    case chat(MirroredSealRoot)
    /// A Temporary session on a device.
    case newThread(teamId: String, deviceId: String, rowIds: [String])
}

/// A failed send, worded per feedback-states.json `errors`.
struct ShareChatError: Error, LocalizedError {
    let title: String
    let underlying: Error

    var errorDescription: String? { "\(title). \(underlying.trpcUserMessage)" }
}

/// EXP-1635: uploads then ONE `sessions.send`, sealed like the app's. ONE
/// submitter is retained for the composer, like `ShareSubmitter`: the uploaded
/// ids survive a failure, and a retry of the same message names the same
/// `clientMessageId`, so the server never posts it twice (F21; a new root's
/// start carries no key, a retry after a lost answer can start a second one).
@MainActor
final class ShareChatSubmitter {
    private let deps: ShareDependencies
    private var target: ShareChatDestination?
    private var images: [UUID: String] = [:]
    private var files: [UUID: SteerImageMessage.File] = [:]
    private var key: (text: String, value: String)?

    init(deps: ShareDependencies) {
        self.deps = deps
    }

    func send(
        accountId: String, destination: ShareChatDestination, text: String, attachments: [ShareAttachment]
    ) async throws {
        if target != destination {
            // Another destination is a new submission: an upload belongs to
            // its team or session, the text to its tree.
            target = destination
            images = [:]
            files = [:]
            key = nil
        }
        do {
            for attachment in attachments where images[attachment.id] == nil && files[attachment.id] == nil {
                let uploaded = try await upload(accountId: accountId, destination: destination, attachment)
                if attachment.isImage {
                    images[attachment.id] = uploaded.id
                } else {
                    files[attachment.id] = SteerImageMessage.File(id: uploaded.id, name: uploaded.filename)
                }
            }
        } catch {
            throw ShareChatError(title: FeedbackStates.Errors.title(verb: "upload", thing: "the attachment"), underlying: error)
        }
        let imageIds = attachments.compactMap { images[$0.id] }
        let fileRows = attachments.compactMap { files[$0.id] }
        let wire = AgentComposerPrompt.build(text: text, attachmentIds: imageIds, files: fileRows)
        let attachmentIds = imageIds + fileRows.map(\.id)
        do {
            try await post(accountId: accountId, destination: destination, text: wire, attachmentIds: attachmentIds)
        } catch {
            let isThread = if case .newThread = destination { true } else { false }
            throw ShareChatError(
                title: FeedbackStates.Errors.title(
                    verb: isThread ? "start" : "send", thing: isThread ? "the session" : "the message"
                ),
                underlying: error
            )
        }
    }

    private func upload(
        accountId: String, destination: ShareChatDestination, _ attachment: ShareAttachment
    ) async throws -> UploadedAttachment {
        switch destination {
        case let .chat(root):
            try await deps.attachmentsApi.uploadSessionImage(
                accountId: accountId, sessionId: root.id,
                data: attachment.data, filename: attachment.filename, contentType: attachment.contentType
            )
        case let .agent(id, conversation):
            try await deps.attachmentsApi.uploadTeamSessionImage(
                accountId: accountId, teamId: conversation?.teamId ?? agentTeamId(accountId: accountId, agentId: id),
                data: attachment.data, filename: attachment.filename, contentType: attachment.contentType
            )
        case let .newThread(teamId, _, _):
            try await deps.attachmentsApi.uploadTeamSessionImage(
                accountId: accountId, teamId: teamId,
                data: attachment.data, filename: attachment.filename, contentType: attachment.contentType
            )
        }
    }

    private func agentTeamId(accountId: String, agentId: String) -> String {
        SharedShareMirror.read(accountId: accountId)?.agents.first { $0.id == agentId }?.teamId ?? ""
    }

    private func post(
        accountId: String, destination: ShareChatDestination, text: String?, attachmentIds: [String]
    ) async throws {
        let attachments = attachmentIds.isEmpty ? nil : attachmentIds
        let input: SessionsSendInput
        let root: SealRoot?
        var rowIds: [String: [String]] = [:]
        switch destination {
        case let .agent(id, conversation):
            input = .agent(agentId: id, text: text, attachmentIds: attachments)
            root = conversation?.sealRoot
        case let .chat(chat):
            input = .message(sessionId: chat.id, text: text, attachmentIds: attachments)
            root = chat.sealRoot
        case let .newThread(teamId, deviceId, ids):
            rowIds[deviceId] = ids
            // EXP-993: a phone never picks a free request's repository: the
            // team's FIRST one anchors it (none = the agent's scratch dir).
            let repositories = try? await deps.repositoriesApi.list(accountId: accountId, teamId: teamId)
            input = .newThread(
                teamId: teamId, deviceId: deviceId, text: text, attachmentIds: attachments,
                repositoryId: repositories?.first?.id
            )
            root = nil
        }
        var prepared = await deps.sealing.prepare(
            accountId: accountId,
            input,
            root: root,
            mentions: nil,
            deviceIds: { id in
                guard let id, !id.isEmpty else { return [] }
                return (rowIds[id] ?? []) + [id]
            }
        )
        prepared.input.clientMessageId = messageKey(for: text ?? "")
        let result: SessionsSendResult = try await deps.trpc.mutation(
            accountId: accountId, path: "sessions.send", input: prepared.input
        )
        await deps.sealing.completed(prepared, result: result)
    }

    /// The same message keeps its key across retries; an edited one is new.
    private func messageKey(for text: String) -> String {
        if let key, key.text == text { return key.value }
        let value = UUID().uuidString
        key = (text, value)
        return value
    }
}
