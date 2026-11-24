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

/// #EXP-2067: a JSON value as it goes out on the wire (a press's card).
public indirect enum WireJSON: Encodable, Equatable, Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([WireJSON])
    case object([String: WireJSON])

    public init?(json: String) {
        guard let value = try? JSONSerialization.jsonObject(with: Data(json.utf8), options: [.fragmentsAllowed]) else { return nil }
        self.init(any: value)
    }

    init(any value: Any) {
        switch value {
        case let number as NSNumber:
            self = CFGetTypeID(number) == CFBooleanGetTypeID() ? .bool(number.boolValue) : .number(number.doubleValue)
        case let string as String: self = .string(string)
        case let array as [Any]: self = .array(array.map(WireJSON.init(any:)))
        case let object as [String: Any]: self = .object(object.mapValues(WireJSON.init(any:)))
        default: self = .null
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case let .bool(value): try c.encode(value)
        case let .number(value):
            if value.isFinite, value == value.rounded(), abs(value) < 1e15 { try c.encode(Int64(value)) } else { try c.encode(value) }
        case let .string(value): try c.encode(value)
        case let .array(values): try c.encode(values)
        case let .object(values): try c.encode(values)
        }
    }
}

/// `sessions.send`'s input plus the sealed-content fields.
public struct SealedSendInput: Encodable, Equatable, Sendable {
    public let input: SessionsSendInput
    public let seal: NewChatSeal?
    public let mentionUserIds: [String]?
    /// EXP-1458: a new child's `name` as a body envelope (the input carries
    /// no clear `name` then).
    public let sealedName: String?
    /// F21: the sender's idempotency key: a retry naming it posts nothing
    /// new (a new root's start carries none server-side).
    public var clientMessageId: String?
    /// #EXP-1958: the sealed card item this send acts on (tRPC only); the
    /// writer passes it through untouched.
    public var cardItem: CardItemRef?
    /// #EXP-2067 (chat-express.json ACTIONS): a press's `action` card (tRPC
    /// only): in clear for a clear tree, else its envelope `{kind, sealed}`.
    public var actionCard: WireJSON?

    public init(
        input: SessionsSendInput,
        seal: NewChatSeal? = nil,
        mentionUserIds: [String]? = nil,
        sealedName: String? = nil,
        clientMessageId: String? = nil,
        cardItem: CardItemRef? = nil,
        actionCard: WireJSON? = nil
    ) {
        self.input = input
        self.seal = seal
        self.mentionUserIds = mentionUserIds
        self.sealedName = sealedName
        self.clientMessageId = clientMessageId
        self.cardItem = cardItem
        self.actionCard = actionCard
    }

    /// #EXP-1958 (`cardStamps`, NEVER IN CLEAR): the writer sealed this
    /// send: its text is an envelope and no clear `name` rides along.
    public var leavesSealed: Bool {
        guard let text = input.text, SealedEnvelope.isSealedBody(text) else { return false }
        return input.name == nil
    }

    private enum Keys: String, CodingKey { case seal, mentionUserIds, sealedName, clientMessageId, cardItem, actionCard }

    public func encode(to encoder: Encoder) throws {
        try input.encode(to: encoder)
        var c = encoder.container(keyedBy: Keys.self)
        try c.encodeIfPresent(seal, forKey: .seal)
        try c.encodeIfPresent(mentionUserIds, forKey: .mentionUserIds)
        try c.encodeIfPresent(sealedName, forKey: .sealedName)
        try c.encodeIfPresent(clientMessageId, forKey: .clientMessageId)
        try c.encodeIfPresent(cardItem, forKey: .cardItem)
        try c.encodeIfPresent(actionCard, forKey: .actionCard)
    }
}

extension SessionsSendInput {
    /// The message text this form carries (nil for the no-text forms).
    public var text: String? {
        switch self {
        case let .newChat(_, _, _, text, _, _, _, _): text
        case let .agent(_, text, _, _, _, _): text
        case let .app(_, _, text, _, _, _, _): text
        case let .child(_, _, _, text, _, _): text
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

    /// A new child's display name (the one form that names one here).
    public var name: String? {
        if case let .child(_, _, _, _, name, _) = self { return name }
        return nil
    }

    /// The same form without its clear `name` (it leaves as `sealedName`).
    func withoutName() -> SessionsSendInput {
        guard case let .child(parentId, deviceId, options, text, _, issueIds) = self else { return self }
        return .child(parentId: parentId, deviceId: deviceId, options: options, text: text, issueIds: issueIds)
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
        case let .child(parentId, deviceId, options, _, name, issueIds):
            .child(parentId: parentId, deviceId: deviceId, options: options, text: text, name: name, issueIds: issueIds)
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
    /// The account a new root's key is recorded for (#EXP-1950 `newRoot`).
    var accountId: String? = nil
}

/// The root a send lands in, as the local database knows it.
struct SealRoot: Equatable, Sendable {
    let id: String
    let teamId: String
    let deviceId: String
    let shared: Bool
    let key: TreeSealKey?
    /// #EDE-97: an outside MCP client's tree: plaintext, never keyed.
    var clearTree: Bool = false
}

public struct SendSealing: Sendable {
    private let holder: TreeKeyHolder
    private let crypto: SealedCrypto
    private let holderWait: Duration
    private let keyWait: Duration
    private let createRetry: Duration

    public init(
        holder: TreeKeyHolder,
        crypto: SealedCrypto,
        holderWait: Duration = sealHolderWait,
        keyWait: Duration = sealKeyWait,
        createRetry: Duration = sealCreateRetry
    ) {
        self.holder = holder
        self.crypto = crypto
        self.holderWait = holderWait
        self.keyWait = keyWait
        self.createRetry = createRetry
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
    /// #EDE-97 (sealed-content.json `clearTree`, `sendWire`): NO SILENT
    /// PLAINTEXT. Only a clear tree takes the text in clear; a holder still
    /// coming up is waited for (`holderWait`), a keyless tree this install
    /// may not key asks its device and waits for its key (`keyWait`,
    /// #EXP-1960 `keylessStep`); anything else
    /// throws `SealRefusal` and nothing leaves. An attachment-only send is not
    /// sealed and never refused.
    func prepare(
        accountId: String,
        _ input: SessionsSendInput,
        root: SealRoot?,
        mentions: [String]?,
        deviceIds: @Sendable (String?) -> [String],
        fresh: Bool = false,
        actionCard: String? = nil
    ) async throws -> PreparedSend {
        let plain = PreparedSend(
            input: SealedSendInput(input: input, mentionUserIds: mentions, actionCard: actionCard.flatMap(WireJSON.init(json:))),
            newKey: nil
        )
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
        case let .message(id, _, _, _), let .child(id, _, _, _, _, _):
            sessionId = id
        }
        // A clear tree takes plaintext and is never keyed.
        if root?.clearTree == true { return plain }
        // #EXP-2067: a press's card leaves sealed like a text.
        let hasText = !(input.text ?? "").isEmpty || !(input.name ?? "").isEmpty || actionCard != nil
        let refuse = { () throws -> PreparedSend in
            guard hasText else { return plain }
            logger.error("No key to seal this send to: refused, nothing sent")
            throw SealRefusal()
        }
        guard await holder.readyInstall(accountId: accountId, wait: hasText ? holderWait : .zero) != nil else {
            return try refuse()
        }
        // #EXP-1950 R2 + R3: an existing root's key only when the rule seals to it.
        let sealed = { (key: TreeSealKey, rootId: String) async throws -> PreparedSend in
            guard await self.holder.sealAllowed(accountId: accountId, rootId: rootId, key: key) else {
                logger.error("The key of this send's tree is refused: nothing sent")
                throw SealRefusal()
            }
            return PreparedSend(input: try self.seal(input, to: key, mentions: mentions, actionCard: actionCard), newKey: nil)
        }
        let target: ServerSendTarget? = if let conversation, root == nil || fresh {
            await holder.sendTarget(accountId: accountId, agentId: conversation.agentId, appId: conversation.appId)
        } else {
            nil
        }
        if target?.clearTree == true { return plain }
        if let target, let rootId = target.rootId {
            // The server's root: sealed to its key, a keyless one keyed first.
            switch await treeKey(
                accountId: accountId,
                rootId: rootId,
                known: target.key,
                teamId: root?.teamId,
                deviceIds: deviceIds(root?.deviceId ?? steerDevice),
                shared: false,
                hasText: hasText
            ) {
            case let .key(key): return try await sealed(key, rootId)
            case .plain: return plain
            case .refused: return try refuse()
            }
        }
        if let root, !(target != nil && fresh) {
            let synced = root.key
            let known = fresh ? (await holder.serverKey(accountId: accountId, rootId: root.id) ?? synced) : synced
            // A store-keyless root is keyed first (`keylessStep` on a failed create).
            switch await treeKey(
                accountId: accountId,
                rootId: root.id,
                known: known,
                teamId: root.teamId,
                deviceIds: deviceIds(root.deviceId),
                shared: root.shared,
                hasText: hasText
            ) {
            case let .key(key): return try await sealed(key, root.id)
            case .plain: return plain
            case .refused: return try refuse()
            }
        }
        if fresh, conversation == nil, let sessionId {
            // A session the store does not have: its key, when it is keyed
            // (or keyed by its device's claim within the wait).
            guard let key = await arrivingKey(accountId: accountId, rootId: sessionId) else { return try refuse() }
            return try await sealed(key, sessionId)
        }
        // A root that does not exist yet: it starts with our key, inline.
        do {
            let (seal, key) = try await holder.newChatSeal(
                accountId: accountId,
                teamId: teamId,
                deviceIds: deviceIds(steerDevice),
                shared: false
            )
            let sealed = try self.seal(
                input, to: TreeSealKey(publicKey: key.publicKey, epoch: 1), mentions: mentions, seal: seal, actionCard: actionCard
            )
            return PreparedSend(input: sealed, newKey: key, accountId: accountId)
        } catch let refusal as SealRefusal {
            throw refusal
        } catch {
            logger.error("No key for a new root (\(String(describing: error), privacy: .public))")
            return try refuse()
        }
    }

    private enum TreeKeyOutcome { case key(TreeSealKey), plain, refused }

    /// #EXP-1960 (sealed-content.json `clearTree.keylessCases`): the key of an
    /// existing root: `known`, else keyed with `sessionKeys.create`; a failed
    /// create takes its `keylessStep` (a blip retried, a non-reader wrap
    /// retried with our own installs, a tree keyed meanwhile sealed to its
    /// key, one this install may not key asked of its device), NEVER a
    /// silent wait. `refused` = nothing can seal it (the send is refused).
    private func treeKey(
        accountId: String,
        rootId: String,
        known: TreeSealKey?,
        teamId: String?,
        deviceIds: [String],
        shared: Bool,
        hasText: Bool
    ) async -> TreeKeyOutcome {
        if let known { return .key(known) }
        var attempt = 1
        var ownOnly = false
        while true {
            let failure: CreateFailure
            switch await holder.createKey(accountId: accountId, rootId: rootId, teamId: teamId, deviceIds: deviceIds, shared: shared, ownOnly: ownOnly) {
            case let .keyed(key): return .key(key)
            case .unavailable: return .refused
            case let .failed(f): failure = f
            }
            // An attachment-only send is never sealed: nothing to wait for.
            guard hasText else { return .refused }
            switch keylessStep(failure: failure, attempt: attempt, ownOnly: ownOnly) {
            case .plain: return .plain
            case .serverKey:
                if let key = await holder.serverKey(accountId: accountId, rootId: rootId) { return .key(key) }
                return await requestedKey(accountId: accountId, rootId: rootId)
            case .requestKey: return await requestedKey(accountId: accountId, rootId: rootId)
            case .retryOwn: ownOnly = true
            case .retry:
                try? await Task.sleep(for: createRetry * attempt)
                if Task.isCancelled { return .refused }
                attempt += 1
            case .failed, .sealed, .wait: return .refused
            }
        }
    }

    /// #EXP-1960: asks the tree's device to key it (`sessionKeys.requestKey`):
    /// `requested` (or no answer) waits for its key, `noHost` fails at once.
    private func requestedKey(accountId: String, rootId: String) async -> TreeKeyOutcome {
        let (answer, key) = await holder.requestKey(accountId: accountId, rootId: rootId)
        switch keylessStep(answer: answer) {
        case .plain: return .plain
        case .sealed:
            if let key { return .key(key) }
        case .wait: break
        default: return .refused
        }
        return await arrivingKey(accountId: accountId, rootId: rootId).map(TreeKeyOutcome.key) ?? .refused
    }

    /// The server's key of a keyless tree this install may not key, polled
    /// until its device's claim keys it (`keyWait`); nil = none came.
    private func arrivingKey(accountId: String, rootId: String) async -> TreeSealKey? {
        let clock = ContinuousClock()
        let deadline = clock.now + keyWait
        while true {
            if let key = await holder.serverKey(accountId: accountId, rootId: rootId) { return key }
            if clock.now >= deadline || Task.isCancelled { return nil }
            try? await Task.sleep(for: min(Self.keyPoll, deadline - clock.now))
        }
    }

    static let keyPoll: Duration = .seconds(1)

    /// The server answered a send: a root started with our key holds it now.
    public func completed(_ prepared: PreparedSend, result: SessionsSendResult) async {
        // A new chat / first conversation answers with its new root.
        guard let key = prepared.newKey else { return }
        await holder.adopt(rootId: result.sessionId, key: key, accountId: prepared.accountId)
    }

    /// The text (and a new child's `name`, as `sealedName`) sealed to
    /// `key`; a seal that fails throws, never plaintext.
    private func seal(
        _ input: SessionsSendInput,
        to key: TreeSealKey,
        mentions: [String]?,
        seal: NewChatSeal? = nil,
        actionCard: String? = nil
    ) throws -> SealedSendInput {
        do {
            var sealed = input
            if let text = input.text, !text.isEmpty {
                let body = SealedAttachmentLines.withEmbeds(text, attachmentIds: input.attachmentIds, uploadOf: SealedAttachmentLines.upload)
                sealed = sealed.with(text: try crypto.sealBody(treePublicKey: key.publicKey, epoch: key.epoch, text: body))
            }
            var sealedName: String?
            if let name = input.name, !name.isEmpty {
                sealedName = try crypto.sealBody(treePublicKey: key.publicKey, epoch: key.epoch, text: name)
                sealed = sealed.withoutName()
            }
            // #EXP-2067: the press's card as its envelope (`{kind, sealed}`).
            let card = try actionCard.map { card -> WireJSON in
                let envelope = try crypto.sealCard(treePublicKey: key.publicKey, epoch: key.epoch, cardJson: card)
                guard let object = WireJSON(json: envelope) else { throw SealRefusal() }
                return object
            }
            return SealedSendInput(input: sealed, seal: seal, mentionUserIds: mentions, sealedName: sealedName, actionCard: card)
        } catch {
            logger.error("Sealing failed: \(String(describing: error), privacy: .public)")
            throw SealRefusal()
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
    /// It runs in the caller's isolation (a main-actor submitter's closure
    /// never crosses actors).
    public static func send<R>(
        isolation: isolated (any Actor)? = #isolation,
        _ attempt: (_ fresh: Bool) async throws -> R
    ) async throws -> R {
        do {
            return try await attempt(false)
        } catch {
            guard isStaleTreeRefusal((error as? LocalizedError)?.errorDescription) else { throw error }
            logger.info("The tree moved on behind this store: re-sealing once")
            return try await attempt(true)
        }
    }
}
