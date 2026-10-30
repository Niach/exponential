import Foundation
import GRDB
import os

// EXP-1459: what a person writes into a session is sealed to the tree's key
// before it leaves the phone (contract addenda 1+2, 2026-10-30):
// - `sessions.send` text = a body envelope; a root without a key (a new
//   thread, a first agent conversation or app editor session) carries the
//   inline `seal {installId, publicKey, epoch: 1, wraps}`; an existing root
//   without one gets it first (`sessionKeys.create`);
// - `mentionUserIds` = the `@email` mentions resolved to user ids (metadata);
// - `sessions.answer` = `sealedCard` (the FULL card re-sealed with its
//   answers) + the clear `decision` / `fresh`.
// Attachments stay clear `attachmentIds`. When nothing can be sealed (no
// eNet in this build, the install not registered yet) the plaintext goes:
// unprefixed bodies stay legal.

private let logger = Logger(subsystem: "at.exponential", category: "SessionSealer")

/// `sessions.send`'s input plus the sealed-content fields.
public struct SealedSendInput: Encodable, Equatable, Sendable {
    public let input: SessionsSendInput
    public let seal: NewThreadSeal?
    public let mentionUserIds: [String]?

    public init(input: SessionsSendInput, seal: NewThreadSeal? = nil, mentionUserIds: [String]? = nil) {
        self.input = input
        self.seal = seal
        self.mentionUserIds = mentionUserIds
    }

    private enum Keys: String, CodingKey { case seal, mentionUserIds }

    public func encode(to encoder: Encoder) throws {
        try input.encode(to: encoder)
        var c = encoder.container(keyedBy: Keys.self)
        try c.encodeIfPresent(seal, forKey: .seal)
        try c.encodeIfPresent(mentionUserIds, forKey: .mentionUserIds)
    }
}

extension SessionsSendInput {
    /// The message text this form carries (nil for the no-text forms).
    public var text: String? {
        switch self {
        case let .newThread(_, _, _, text, _, _, _, _): text
        case let .agent(_, text, _, _): text
        case let .app(_, _, text, _): text
        case let .child(_, _, _, text): text
        case let .message(_, text, _): text
        case .switchAccount, .moveDevice: nil
        }
    }

    /// The same form with `text` replaced.
    public func with(text: String?) -> SessionsSendInput {
        switch self {
        case let .newThread(teamId, deviceId, options, _, attachmentIds, issueIds, promptId, repositoryId):
            .newThread(teamId: teamId, deviceId: deviceId, options: options, text: text, attachmentIds: attachmentIds, issueIds: issueIds, promptId: promptId, repositoryId: repositoryId)
        case let .agent(agentId, _, attachmentIds, issueIds):
            .agent(agentId: agentId, text: text, attachmentIds: attachmentIds, issueIds: issueIds)
        case let .app(appId, deviceId, _, attachmentIds):
            .app(appId: appId, deviceId: deviceId, text: text, attachmentIds: attachmentIds)
        case let .child(parentId, deviceId, options, _):
            .child(parentId: parentId, deviceId: deviceId, options: options, text: text)
        case let .message(sessionId, _, attachmentIds):
            .message(sessionId: sessionId, text: text, attachmentIds: attachmentIds)
        case .switchAccount, .moveDevice:
            self
        }
    }
}

/// `sessions.answer` with the card sealed: only the clear part is checked
/// server-side.
struct SealedAnswerInput: Encodable {
    let messageId: String
    let decision: PlanDecision?
    let fresh: Bool?
    let sealedCard: CardJSON
}

/// A send the sealer prepared, plus the key a new root starts with.
public struct PreparedSend: Sendable {
    public let input: SealedSendInput
    let newKey: GeneratedTreeKey?
}

/// Pure: the `@email` mentions of a text, lowercased, in order, once each
/// (the editor writes them as plain `@<email>` tokens).
public enum SealedMentions {
    private static let pattern = try! NSRegularExpression(pattern: #"(?<![\w.+-])@([A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,})"#)

    public static func emails(in text: String) -> [String] {
        let range = NSRange(text.startIndex..., in: text)
        var seen = Set<String>()
        var out: [String] = []
        for match in pattern.matches(in: text, range: range) {
            guard let r = Range(match.range(at: 1), in: text) else { continue }
            var email = String(text[r]).lowercased()
            while email.hasSuffix(".") { email.removeLast() }
            if seen.insert(email).inserted { out.append(email) }
        }
        return out
    }
}

/// The root a send lands in, as the local database knows it.
struct SealRoot: Equatable {
    let id: String
    let teamId: String
    let deviceId: String
    let shared: Bool
    let key: TreeSealKey?
}

public final class SessionSealer: @unchecked Sendable {
    private let holder: TreeKeyHolder
    private let crypto: SealedCrypto
    private let opener: SealedOpener
    private let db: DatabaseManager
    private let userId: @Sendable (String) -> String?

    public init(holder: TreeKeyHolder, opener: SealedOpener, db: DatabaseManager, userId: @escaping @Sendable (String) -> String?) {
        self.holder = holder
        self.crypto = opener.crypto
        self.opener = opener
        self.db = db
        self.userId = userId
    }

    private func read<T>(_ accountId: String, _ body: (Database) throws -> T?) -> T? {
        guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
        return (try? pool.read { try body($0) }) ?? nil
    }

    /// The root of an existing session / the caller's conversation root.
    func root(accountId: String, for input: SessionsSendInput) -> SealRoot? {
        let me = userId(accountId)
        return read(accountId) { db -> SealRoot? in
            var root: SessionEntity?
            switch input {
            case let .message(sessionId, _, _), let .child(sessionId, _, _, _):
                if let session = try SessionEntity.fetchOne(db, key: sessionId) {
                    root = try SessionEntity.fetchOne(db, key: session.rootId)
                }
            case let .agent(agentId, _, _, _):
                guard let me else { return nil }
                root = try SessionEntity.fetchOne(db, sql: """
                    SELECT * FROM "sessions" WHERE "agent_id" = ? AND "user_id" = ? AND "parent_id" IS NULL
                    ORDER BY "created_at" DESC LIMIT 1
                    """, arguments: [agentId, me])
            case let .app(appId, _, _, _):
                guard let me else { return nil }
                root = try SessionEntity.fetchOne(db, sql: """
                    SELECT * FROM "sessions" WHERE "app_id" = ? AND "user_id" = ? AND "parent_id" IS NULL
                    ORDER BY "created_at" DESC LIMIT 1
                    """, arguments: [appId, me])
            case .newThread, .switchAccount, .moveDevice:
                return nil
            }
            guard let root else { return nil }
            return SealRoot(id: root.id, teamId: root.teamId, deviceId: root.deviceId, shared: root.shared, key: root.sealKey)
        }
    }

    /// The ids an install's `deviceId` may name for a steer device id (the
    /// `devices` row ids, plus the steer id itself).
    func deviceIds(accountId: String, steerDeviceId: String?) -> [String] {
        guard let steerDeviceId, !steerDeviceId.isEmpty else { return [] }
        let rows = read(accountId) { db in
            try String.fetchAll(db, sql: #"SELECT "id" FROM "devices" WHERE "device_id" = ?"#, arguments: [steerDeviceId])
        } ?? []
        return rows + [steerDeviceId]
    }

    /// The mentioned people's user ids (members this database knows).
    func mentionUserIds(accountId: String, text: String?) -> [String]? {
        guard let text else { return nil }
        let emails = SealedMentions.emails(in: text)
        guard !emails.isEmpty else { return nil }
        let ids = read(accountId) { db -> [String] in
            let marks = Array(repeating: "?", count: emails.count).joined(separator: ", ")
            return try String.fetchAll(db, sql: #"SELECT "id" FROM "users" WHERE lower("email") IN (\#(marks))"#, arguments: StatementArguments(emails))
        } ?? []
        return ids.isEmpty ? nil : ids.sorted()
    }

    /// Seals the text of a send (and keys the root when it has no key).
    public func prepare(accountId: String, _ input: SessionsSendInput) async -> PreparedSend {
        let mentions = mentionUserIds(accountId: accountId, text: input.text)
        guard crypto.isAvailable else { return PreparedSend(input: SealedSendInput(input: input, mentionUserIds: mentions), newKey: nil) }
        switch input {
        case .switchAccount, .moveDevice:
            return PreparedSend(input: SealedSendInput(input: input), newKey: nil)
        default:
            break
        }
        if let root = root(accountId: accountId, for: input) {
            let key = await holder.sealKey(
                accountId: accountId,
                rootId: root.id,
                known: root.key,
                teamId: root.teamId,
                deviceIds: deviceIds(accountId: accountId, steerDeviceId: root.deviceId),
                shared: root.shared
            )
            return PreparedSend(input: SealedSendInput(input: seal(input, to: key), mentionUserIds: mentions), newKey: nil)
        }
        // A root that does not exist yet: it starts with our key, inline.
        var teamId: String?
        var steerDevice: String?
        switch input {
        case let .newThread(team, device, _, _, _, _, _, _):
            teamId = team
            steerDevice = device
        case let .app(_, device, _, _):
            steerDevice = device
        default:
            break
        }
        do {
            let (seal, key) = try await holder.newThreadSeal(
                accountId: accountId,
                teamId: teamId,
                deviceIds: deviceIds(accountId: accountId, steerDeviceId: steerDevice),
                shared: false
            )
            let sealed = self.seal(input, to: TreeSealKey(publicKey: key.publicKey, epoch: 1))
            return PreparedSend(input: SealedSendInput(input: sealed, seal: seal, mentionUserIds: mentions), newKey: key)
        } catch {
            logger.error("No key for a new root (\(String(describing: error), privacy: .public)): sending plaintext")
            return PreparedSend(input: SealedSendInput(input: input, mentionUserIds: mentions), newKey: nil)
        }
    }

    /// The server answered a send: a root started with our key holds it now.
    public func completed(_ prepared: PreparedSend, result: SessionsSendResult) async {
        // A new thread / first conversation answers with its new root.
        guard let key = prepared.newKey else { return }
        await holder.adopt(rootId: result.sessionId, key: key)
    }

    private func seal(_ input: SessionsSendInput, to key: TreeSealKey?) -> SessionsSendInput {
        guard let key, let text = input.text, !text.isEmpty else { return input }
        do {
            return input.with(text: try crypto.sealBody(treePublicKey: key.publicKey, epoch: key.epoch, text: text))
        } catch {
            logger.error("Sealing failed: \(String(describing: error), privacy: .public)")
            return input
        }
    }

    // MARK: answers

    /// The sealed form of an answer when the card is sealed (nil = answer in
    /// clear, the card is plaintext or can't be resealed here).
    func sealAnswer(accountId: String, _ input: AnswerMessageInput) async -> SealedAnswerInput? {
        guard crypto.isAvailable else { return nil }
        let row = read(accountId) { db -> (card: String, rootId: String)? in
            guard let message = try MessageEntity.fetchOne(db, key: input.messageId), let card = message.card else { return nil }
            return (card, message.rootId)
        }
        guard let row, SealedEnvelope.mayBeSealedCard(row.card) else { return nil }
        let opened = opener.card(row.card, rootId: row.rootId)
        guard opened.status == .opened,
              let card = (try? JSONSerialization.jsonObject(with: Data(opened.json.utf8))) as? [String: Any] else { return nil }
        let root = read(accountId) { db in try SessionEntity.fetchOne(db, key: row.rootId) }
        guard let key = await holder.sealKey(
            accountId: accountId,
            rootId: row.rootId,
            known: root?.sealKey,
            teamId: root?.teamId,
            deviceIds: [],
            shared: root?.shared ?? false
        ) else { return nil }
        let answered = SealedAnswer.answeredCard(card, input)
        guard let data = try? JSONSerialization.data(withJSONObject: answered, options: [.sortedKeys]),
              let json = String(data: data, encoding: .utf8),
              let envelope = try? crypto.sealCard(treePublicKey: key.publicKey, epoch: key.epoch, cardJson: json),
              let object = try? JSONSerialization.jsonObject(with: Data(envelope.utf8)),
              let sealedCard = CardJSON(any: object) else { return nil }
        return SealedAnswerInput(messageId: input.messageId, decision: input.decision, fresh: input.fresh, sealedCard: sealedCard)
    }
}

/// The answered card, as the server stamps it in clear
/// (apps/web/src/lib/sessions/card-answer.ts `answeredCard`).
public enum SealedAnswer {
    static func answeredCard(_ card: [String: Any], _ input: AnswerMessageInput) -> [String: Any] {
        var out = card
        if let surfaceData = input.surfaceData { out["surfaceData"] = surfaceData.any }
        if card["kind"] as? String == "question" {
            var labels: [String] = []
            if case let .question(picked)? = input.answers {
                labels = picked.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
            }
            let answers = labels.isEmpty ? [(input.answer ?? "").trimmingCharacters(in: .whitespacesAndNewlines)] : labels
            out["answer"] = labels.isEmpty ? answers[0] : PlanCard.joinAnswers(labels)
            out["answers"] = answers
            return out
        }
        out["decision"] = input.decision?.rawValue
        out["fresh"] = input.decision == .approved && input.fresh == true
        let feedback = input.feedback?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        out["feedback"] = feedback.isEmpty ? NSNull() : feedback
        if case let .plan(picks)? = input.answers {
            var answers: [String: [String]] = [:]
            for (id, labels) in picks {
                let picked = labels.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
                if !picked.isEmpty { answers[id] = picked }
            }
            out["answers"] = answers
        }
        return out
    }
}

/// Where `SessionsApi` finds the sealer (set once at launch, after the key
/// holder exists).
public final class SessionSealerSlot: @unchecked Sendable {
    private let lock = NSLock()
    private var value: SessionSealer?

    public init() {}

    public var sealer: SessionSealer? {
        get { lock.withLock { value } }
        set { lock.withLock { value = newValue } }
    }
}
