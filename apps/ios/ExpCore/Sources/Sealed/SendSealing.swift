import Foundation
import os

// EXP-1459: the writer's half of sealed content, free of the local database:
// what a send needs (the root and its readers) comes from the caller, so the
// app (`SessionSealer`, reading GRDB) and the share extension (reading the
// app-group mirror, EXP-1635) seal exactly alike.

private let logger = Logger(subsystem: "at.exponential", category: "SendSealing")

/// #EXP-1683 (sealed-content.json `attachmentEmbedCases`): the server never
/// appends to a sealed body, so the writer names a send's attachments inside
/// the text before sealing; a file is a link line, never an image embed.
public enum SealedAttachmentLines {
    public struct Upload: Sendable, Equatable {
        public let filename: String
        public let contentType: String

        public init(filename: String, contentType: String) {
            self.filename = filename
            self.contentType = contentType
        }
    }

    private static let uploads = OSAllocatedUnfairLock(initialState: [String: Upload]())

    /// Recorded by the session upload routes (`AttachmentsApi`).
    public static func remember(id: String, upload: Upload) {
        uploads.withLock { $0[id] = upload }
    }

    public static func upload(id: String) -> Upload? {
        uploads.withLock { $0[id] }
    }

    public static func withEmbeds(_ text: String, attachmentIds: [String], uploadOf: (String) -> Upload?) -> String {
        var seen = Set<String>()
        let lines = attachmentIds.filter { seen.insert($0).inserted && !text.contains("/api/attachments/\($0)") }.map { id in
            let upload = uploadOf(id)
            let name = (upload?.filename ?? "image").replacingOccurrences(of: "[", with: "").replacingOccurrences(of: "]", with: "")
            let image = upload.map { AttachmentFiles.isInlineImage(contentType: $0.contentType) } ?? true
            return "\(image ? "!" : "")[\(name)](/api/attachments/\(id))"
        }
        if lines.isEmpty { return text }
        var trimmed = text
        while trimmed.last?.isWhitespace == true { trimmed.removeLast() }
        return ([trimmed] + lines).filter { !$0.isEmpty }.joined(separator: "\n\n")
    }
}

/// `sessions.send`'s input plus the sealed-content fields.
public struct SealedSendInput: Encodable, Equatable, Sendable {
    public let input: SessionsSendInput
    public let seal: NewChatSeal?
    public let mentionUserIds: [String]?
    /// F21: the sender's idempotency key: a retry naming it posts nothing
    /// new (a new root's start carries none server-side).
    public var clientMessageId: String?

    public init(input: SessionsSendInput, seal: NewChatSeal? = nil, mentionUserIds: [String]? = nil, clientMessageId: String? = nil) {
        self.input = input
        self.seal = seal
        self.mentionUserIds = mentionUserIds
        self.clientMessageId = clientMessageId
    }

    private enum Keys: String, CodingKey { case seal, mentionUserIds, clientMessageId }

    public func encode(to encoder: Encoder) throws {
        try input.encode(to: encoder)
        var c = encoder.container(keyedBy: Keys.self)
        try c.encodeIfPresent(seal, forKey: .seal)
        try c.encodeIfPresent(mentionUserIds, forKey: .mentionUserIds)
        try c.encodeIfPresent(clientMessageId, forKey: .clientMessageId)
    }
}

extension SessionsSendInput {
    /// The message text this form carries (nil for the no-text forms).
    public var text: String? {
        switch self {
        case let .newChat(_, _, _, text, _, _, _, _): text
        case let .agent(_, text, _, _, _, _): text
        case let .app(_, _, text, _, _, _, _): text
        case let .child(_, _, _, text): text
        case let .message(_, text, _, _): text
        case .switchAccount, .moveDevice: nil
        }
    }

    /// The claimed uploads of this send.
    public var attachmentIds: [String] {
        switch self {
        case let .newChat(_, _, _, _, ids, _, _, _): ids ?? []
        case let .agent(_, _, ids, _, _, _): ids ?? []
        case let .app(_, _, _, ids, _, _, _): ids ?? []
        case let .message(_, _, ids, _): ids ?? []
        case .child, .switchAccount, .moveDevice: []
        }
    }

    /// The same form with `text` replaced.
    public func with(text: String?) -> SessionsSendInput {
        switch self {
        case let .newChat(teamId, deviceId, options, _, attachmentIds, issueIds, promptId, repositoryId):
            .newChat(teamId: teamId, deviceId: deviceId, options: options, text: text, attachmentIds: attachmentIds, issueIds: issueIds, promptId: promptId, repositoryId: repositoryId)
        case let .agent(agentId, _, attachmentIds, issueIds, scope, mcpServerIds):
            .agent(agentId: agentId, text: text, attachmentIds: attachmentIds, issueIds: issueIds, scope: scope, mcpServerIds: mcpServerIds)
        case let .app(appId, deviceId, _, attachmentIds, cli, account, model):
            .app(appId: appId, deviceId: deviceId, text: text, attachmentIds: attachmentIds, cli: cli, account: account, model: model)
        case let .child(parentId, deviceId, options, _):
            .child(parentId: parentId, deviceId: deviceId, options: options, text: text)
        case let .message(sessionId, _, attachmentIds, scope):
            .message(sessionId: sessionId, text: text, attachmentIds: attachmentIds, scope: scope)
        case .switchAccount, .moveDevice:
            self
        }
    }
}

/// A send the sealer prepared, plus the key a new root starts with.
public struct PreparedSend: Sendable {
    public var input: SealedSendInput
    let newKey: GeneratedTreeKey?
}

/// The root a send lands in, as the local database knows it.
struct SealRoot: Equatable, Sendable {
    let id: String
    let teamId: String
    let deviceId: String
    let shared: Bool
    let key: TreeSealKey?
}

public struct SendSealing: Sendable {
    private let holder: TreeKeyHolder
    private let crypto: SealedCrypto

    public init(holder: TreeKeyHolder, crypto: SealedCrypto) {
        self.holder = holder
        self.crypto = crypto
    }

    /// Seals the text of a send (and keys the root when it has no key).
    /// `root` = the root it lands in as the store knows it, nil = one the
    /// store does not have; `deviceIds` = the ids an install's `deviceId` may
    /// name for a steer device id.
    /// #FEED-250 (sealed-content.json `staleTree`): the store is a cache. A
    /// conversation with an agent / app it lacks is asked of the server
    /// (`sessions.sendTarget`) before a key rides inline; `fresh` = the server
    /// refused this send's key write once: the tree is resolved from the
    /// server alone.
    func prepare(
        accountId: String,
        _ input: SessionsSendInput,
        root: SealRoot?,
        mentions: [String]?,
        deviceIds: @Sendable (String?) -> [String],
        fresh: Bool = false
    ) async -> PreparedSend {
        let plain = PreparedSend(input: SealedSendInput(input: input, mentionUserIds: mentions), newKey: nil)
        guard crypto.isAvailable else { return plain }
        var teamId: String?
        var steerDevice: String?
        var conversation: (agentId: String?, appId: String?)?
        var sessionId: String?
        switch input {
        case .switchAccount, .moveDevice:
            return PreparedSend(input: SealedSendInput(input: input), newKey: nil)
        case let .newChat(team, device, _, _, _, _, _, _):
            teamId = team
            steerDevice = device
        case let .agent(agentId, _, _, _, _, _):
            conversation = (agentId, nil)
        case let .app(appId, device, _, _, _, _, _):
            conversation = (nil, appId)
            steerDevice = device
        case let .message(id, _, _, _), let .child(id, _, _, _):
            sessionId = id
        }
        let sealed = { (key: TreeSealKey) in
            PreparedSend(input: SealedSendInput(input: self.seal(input, to: key), mentionUserIds: mentions), newKey: nil)
        }
        let target: ServerSendTarget? = if let conversation, root == nil || fresh {
            await holder.sendTarget(accountId: accountId, agentId: conversation.agentId, appId: conversation.appId)
        } else {
            nil
        }
        if let target, let rootId = target.rootId {
            // The server's root: sealed to its key, a keyless one keyed first.
            guard let key = await holder.sealKey(
                accountId: accountId,
                rootId: rootId,
                known: target.key,
                teamId: root?.teamId,
                deviceIds: deviceIds(root?.deviceId ?? steerDevice),
                shared: false,
                lookup: false
            ) else { return plain }
            return sealed(key)
        }
        if let root, !(target != nil && fresh) {
            let synced = root.key
            let known = fresh ? (await holder.serverKey(accountId: accountId, rootId: root.id) ?? synced) : synced
            // A store-keyless root is keyed first; a refused create asks the
            // server's key before the plaintext goes.
            guard let key = await holder.sealKey(
                accountId: accountId,
                rootId: root.id,
                known: known,
                teamId: root.teamId,
                deviceIds: deviceIds(root.deviceId),
                shared: root.shared,
                lookup: false
            ) else { return plain }
            return sealed(key)
        }
        if fresh, conversation == nil, let sessionId {
            // A session the store does not have: its key, when it is a keyed root.
            guard let key = await holder.serverKey(accountId: accountId, rootId: sessionId) else { return plain }
            return sealed(key)
        }
        // A root that does not exist yet: it starts with our key, inline.
        do {
            let (seal, key) = try await holder.newChatSeal(
                accountId: accountId,
                teamId: teamId,
                deviceIds: deviceIds(steerDevice),
                shared: false
            )
            let sealed = self.seal(input, to: TreeSealKey(publicKey: key.publicKey, epoch: 1))
            return PreparedSend(input: SealedSendInput(input: sealed, seal: seal, mentionUserIds: mentions), newKey: key)
        } catch {
            logger.error("No key for a new root (\(String(describing: error), privacy: .public)): sending plaintext")
            return plain
        }
    }

    /// The server answered a send: a root started with our key holds it now.
    public func completed(_ prepared: PreparedSend, result: SessionsSendResult) async {
        // A new chat / first conversation answers with its new root.
        guard let key = prepared.newKey else { return }
        await holder.adopt(rootId: result.sessionId, key: key)
    }

    private func seal(_ input: SessionsSendInput, to key: TreeSealKey?) -> SessionsSendInput {
        guard let key, let text = input.text, !text.isEmpty else { return input }
        do {
            let body = SealedAttachmentLines.withEmbeds(text, attachmentIds: input.attachmentIds, uploadOf: SealedAttachmentLines.upload)
            return input.with(text: try crypto.sealBody(treePublicKey: key.publicKey, epoch: key.epoch, text: body))
        } catch {
            logger.error("Sealing failed: \(String(describing: error), privacy: .public)")
            return input
        }
    }
}

/// #FEED-250 (sealed-content.json `staleTree`): a `sessions.send` whose key
/// write the server refused (the tree moved on behind this install's store)
/// is re-resolved from the server, re-sealed and re-sent ONCE by itself, the
/// first refusal never surfaced; a second one throws (its description = the
/// refusal, `TrpcErrorBody.presentableMessage`).
public enum StaleTreeResend {
    /// `attempt(fresh)` prepares and sends; `fresh` = resolve from the server.
    public static func send<R>(_ attempt: (_ fresh: Bool) async throws -> R) async throws -> R {
        do {
            return try await attempt(false)
        } catch {
            guard isStaleTreeRefusal((error as? LocalizedError)?.errorDescription) else { throw error }
            logger.info("The tree moved on behind this store: re-sealing once")
            return try await attempt(true)
        }
    }
}
