import Foundation
import GRDB

// EXP-1459: what a person writes into a session is sealed to the tree's key
// before it leaves the phone (contract addenda 1+2, 2026-10-30):
// - `sessions.send` text = a body envelope; a root without a key (a new
//   chat, a first agent conversation or app editor session) carries the
//   inline `seal {installId, publicKey, epoch: 1, wraps}`; an existing root
//   without one gets it first (`sessionKeys.create`);
// - `mentionUserIds` = the `@email` mentions resolved to user ids (metadata);
// - `refIssueIds` = the `#IDENT` refs resolved over the chat teams (#EXP-2093);
// - `sessions.answer` = `sealedCard` (the FULL card re-sealed with its
//   answers) + the clear `decision` / `fresh`.
// Attachments stay clear `attachmentIds`. #EDE-97: text goes in clear only
// into a clear tree (an outside MCP client's); when nothing can be sealed the
// send fails (`SealRefusal`, sealed-content.json `clearTree`).


/// `sessions.answer` with the card sealed: only the clear part is checked
/// server-side.
struct SealedAnswerInput: Encodable {
    let messageId: String
    let decision: PlanDecision?
    let fresh: Bool?
    let sealedCard: CardJSON
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

/// #EXP-2093: the `#IDENT` issue refs a sealed text carries as
/// `refIssueIds` (web `refIssueIds`, Android `SealedWrite.issueRefs`).
public enum SealedIssueRefs {
    /// ExpUI `IssueRefs.pattern`'s `#` form (web `issue-refs.ts`).
    private static let regex = try! NSRegularExpression(pattern: "(?<![A-Za-z0-9_#])#([A-Za-z][A-Za-z0-9]*-[0-9]+)(?![A-Za-z0-9_-])")

    /// The identifiers of a text, uppercased, in order, once each.
    public static func identifiers(in text: String) -> [String] {
        var seen = Set<String>()
        var out: [String] = []
        for match in regex.matches(in: text, range: NSRange(text.startIndex..., in: text)) {
            guard let r = Range(match.range(at: 1), in: text) else { continue }
            let identifier = text[r].uppercased()
            if seen.insert(identifier).inserted { out.append(identifier) }
        }
        return out
    }

    /// The ids of `issues` (each with its team) the `identifiers` name inside
    /// `teamIds`; an identifier in several teams = the first team's in order.
    public static func resolve(
        identifiers: [String], issues: [(id: String, identifier: String, teamId: String)], teamIds: [String]
    ) -> [String] {
        let rank = Dictionary(teamIds.enumerated().map { ($1, $0) }, uniquingKeysWith: { a, _ in a })
        var best: [String: (rank: Int, id: String)] = [:]
        for issue in issues {
            guard let at = rank[issue.teamId] else { continue }
            let key = issue.identifier.uppercased()
            if let held = best[key], held.rank <= at { continue }
            best[key] = (at, issue.id)
        }
        return identifiers.compactMap { best[$0]?.id }
    }
}

public final class SessionSealer: @unchecked Sendable {
    private let holder: TreeKeyHolder
    private let crypto: SealedCrypto
    private let opener: SealedOpener
    private let sending: SendSealing
    private let db: DatabaseManager
    private let userId: @Sendable (String) -> String?

    public init(holder: TreeKeyHolder, opener: SealedOpener, db: DatabaseManager, userId: @escaping @Sendable (String) -> String?) {
        self.holder = holder
        self.crypto = opener.crypto
        self.opener = opener
        self.sending = SendSealing(holder: holder, crypto: opener.crypto)
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
            case let .message(sessionId, _, _, _), let .child(sessionId, _, _, _, _, _):
                if let session = try SessionEntity.fetchOne(db, key: sessionId) {
                    root = try SessionEntity.fetchOne(db, key: session.rootId)
                }
            case let .agent(agentId, _, _, _, _, _):
                // #EXP-1990 (`agentChatOf`): a team agent's group chat (its
                // key, whoever started it), else my own conversation; none
                // = the first message creates the key.
                root = try AgentSettings.agentChat(db: db, agentId: agentId, me: me)
            case let .app(appId, _, _, _, _, _, _):
                guard let me else { return nil }
                root = try SessionEntity.fetchOne(db, sql: """
                    SELECT * FROM "sessions" WHERE "app_id" = ? AND "user_id" = ? AND "parent_id" IS NULL
                    ORDER BY "created_at" DESC LIMIT 1
                    """, arguments: [appId, me])
            case .newChat, .switchAccount, .moveDevice:
                return nil
            }
            guard let root else { return nil }
            return SealRoot(
                id: root.id, teamId: root.teamId, deviceId: root.deviceId, shared: root.shared, key: root.sealKey,
                clearTree: root.clearTree, scopeTeamIds: root.scopeTeamIds
            )
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

    /// #EXP-2093 (agent-settings.json `assistant.chatTeams` (1)): the issues
    /// the text's `#IDENT`s name, over the CHAT TEAMS of the tree it lands in
    /// (a team chat's one team; the Assistant's: this send's Teams pick, else
    /// the root's, ∩ the synced teams, else all of them); an identifier found
    /// in several = the first team's in order. Nil = none.
    func refIssueIds(accountId: String, _ input: SessionsSendInput, root: SealRoot?) -> [String]? {
        guard let text = input.text else { return nil }
        let identifiers = SealedIssueRefs.identifiers(in: text)
        guard !identifiers.isEmpty else { return nil }
        let ids = read(accountId) { db -> [String] in
            let synced = TeamGroups.ordered(try TeamEntity.fetchAll(db)).map(\.id)
            var own = root.flatMap { $0.teamId.isEmpty ? nil : $0.teamId }
            if own == nil {
                switch input {
                case let .newChat(teamId, _, _, _, _, _, _, _): own = teamId
                case let .agent(agentId, _, _, _, _, _):
                    own = try String.fetchOne(db, sql: #"SELECT "team_id" FROM "agents" WHERE "id" = ?"#, arguments: [agentId])
                case let .app(appId, _, _, _, _, _, _):
                    own = try String.fetchOne(db, sql: #"SELECT "team_id" FROM "apps" WHERE "id" = ?"#, arguments: [appId])
                default: break
                }
            }
            // The turn's teams: the pick this send carries, else the root's.
            let picked = input.scope.map(\.picked) ?? root?.scopeTeamIds
            let teams = AgentSettings.chatTeams(ownTeamId: own, syncedTeamIds: synced, pickedTeamIds: picked)
            guard !teams.isEmpty else { return [] }
            let marks = { (n: Int) in Array(repeating: "?", count: n).joined(separator: ", ") }
            let rows = try Row.fetchAll(db, sql: """
                SELECT i."id", i."identifier", b."team_id" FROM "issues" i JOIN "boards" b ON b."id" = i."board_id"
                WHERE upper(i."identifier") IN (\(marks(identifiers.count))) AND b."team_id" IN (\(marks(teams.count)))
                """, arguments: StatementArguments(identifiers + teams))
            return SealedIssueRefs.resolve(
                identifiers: identifiers,
                issues: rows.map { (id: $0["id"], identifier: $0["identifier"], teamId: $0["team_id"]) },
                teamIds: teams
            )
        } ?? []
        return ids.isEmpty ? nil : ids
    }

    /// Seals the text of a send (and keys the root when it has no key).
    /// `fresh` = the server refused this send's key write once: the tree is
    /// resolved from the server, never this store (#FEED-250 `staleTree`).
    /// #EXP-2067: `actionCard` = a press's card, sealed with the text.
    public func prepare(
        accountId: String, _ input: SessionsSendInput, fresh: Bool = false, actionCard: String? = nil
    ) async throws -> PreparedSend {
        let root = root(accountId: accountId, for: input)
        var prepared = try await sending.prepare(
            accountId: accountId,
            input,
            root: root,
            mentions: mentionUserIds(accountId: accountId, text: input.text),
            deviceIds: { deviceIds(accountId: accountId, steerDeviceId: $0) },
            fresh: fresh,
            actionCard: actionCard
        )
        if prepared.input.input.text != nil {
            prepared.input.refIssueIds = refIssueIds(accountId: accountId, input, root: root)
        }
        return prepared
    }

    /// #EXP-1641: a send into the ENCRYPTED tree `rootId` (a notification's
    /// reply), sealed even when the session row is not local; nil = it could
    /// not be sealed here (the caller sends nothing).
    public func prepareSealed(accountId: String, _ input: SessionsSendInput, rootId: String) async -> PreparedSend? {
        guard crypto.isAvailable, let text = input.text, !text.isEmpty else { return nil }
        let local = root(accountId: accountId, for: input)
        let target = local?.id == rootId ? local! : SealRoot(id: rootId, teamId: "", deviceId: "", shared: false, key: nil)
        guard let key = await holder.sealKey(
            accountId: accountId, rootId: target.id, known: target.key, teamId: target.teamId,
            deviceIds: deviceIds(accountId: accountId, steerDeviceId: target.deviceId), shared: target.shared
        ) else { return nil }
        guard let sealed = try? crypto.sealBody(
            treePublicKey: key.publicKey, epoch: key.epoch,
            text: SealedAttachmentLines.withEmbeds(text, attachmentIds: input.attachmentIds, uploadOf: SealedAttachmentLines.upload)
        ), SealedEnvelope.isSealedBody(sealed) else { return nil }
        return PreparedSend(
            input: SealedSendInput(
                input: input.with(text: sealed),
                mentionUserIds: mentionUserIds(accountId: accountId, text: text),
                refIssueIds: refIssueIds(accountId: accountId, input, root: local?.id == rootId ? local : nil)
            ),
            newKey: nil
        )
    }

    /// The server answered a send: a root started with our key holds it now.
    public func completed(_ prepared: PreparedSend, result: SessionsSendResult) async {
        await sending.completed(prepared, result: result)
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
        // An escalated card this install can't open in its own tree: the
        // mirror's `sealedCard` (sealed to the watching tree) stands in.
        let json = opened.status == .opened ? opened.json : escalatedCard(accountId: accountId, messageId: input.messageId)
        guard let json,
              let card = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [String: Any] else { return nil }
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

    /// The opened `sealedCard` of an `ask` mirror escalating `messageId`.
    private func escalatedCard(accountId: String, messageId: String) -> String? {
        let mirrors = read(accountId) { db in
            try MessageEntity
                .filter(Column("card").like("%\"sealedCard\"%") && Column("card").like("%\(messageId)%"))
                .fetchAll(db)
        } ?? []
        for mirror in mirrors {
            guard let object = (try? JSONSerialization.jsonObject(with: Data((mirror.card ?? "").utf8))) as? [String: Any],
                  object["messageId"] as? String == messageId,
                  let json = SealedMessage.escalatedCard(mirror.card, rootId: mirror.rootId, opener: opener)
            else { continue }
            return json
        }
        return nil
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
/// #EXP-2093: the `#IDENT` issue refs a sealed text carries as
/// `refIssueIds` (web `refIssueIds`, Android `SealedWrite.issueRefs`).
public enum SealedIssueRefs {
    /// ExpUI `IssueRefs.pattern`'s `#` form (web `issue-refs.ts`).
    private static let regex = try! NSRegularExpression(pattern: "(?<![A-Za-z0-9_#])#([A-Za-z][A-Za-z0-9]*-[0-9]+)(?![A-Za-z0-9_-])")

    /// The identifiers of a text, uppercased, in order, once each.
    public static func identifiers(in text: String) -> [String] {
        var seen = Set<String>()
        var out: [String] = []
        for match in regex.matches(in: text, range: NSRange(text.startIndex..., in: text)) {
            guard let r = Range(match.range(at: 1), in: text) else { continue }
            let identifier = text[r].uppercased()
            if seen.insert(identifier).inserted { out.append(identifier) }
        }
        return out
    }

    /// The ids of `issues` (each with its team) the `identifiers` name inside
    /// `teamIds`; an identifier in several teams = the first team's in order.
    public static func resolve(
        identifiers: [String], issues: [(id: String, identifier: String, teamId: String)], teamIds: [String]
    ) -> [String] {
        let rank = Dictionary(teamIds.enumerated().map { ($1, $0) }, uniquingKeysWith: { a, _ in a })
        var best: [String: (rank: Int, id: String)] = [:]
        for issue in issues {
            guard let at = rank[issue.teamId] else { continue }
            let key = issue.identifier.uppercased()
            if let held = best[key], held.rank <= at { continue }
            best[key] = (at, issue.id)
        }
        return identifiers.compactMap { best[$0]?.id }
    }
}

public final class SessionSealerSlot: @unchecked Sendable {
    private let lock = NSLock()
    private var value: SessionSealer?

    public init() {}

    public var sealer: SessionSealer? {
        get { lock.withLock { value } }
        set { lock.withLock { value = newValue } }
    }
}

extension SessionEntity {
    /// The display title: the opened `sealed_title`, else the clear `title`.
    public var openedTitle: String? {
        SealedOpener.shared.title(sealedTitle: sealedTitle, title: title, rootId: rootId)
    }

    /// The key a writer seals to, when this root row carries one.
    public var sealKey: TreeSealKey? {
        guard let sealPublicKey, let sealEpoch, sealEpoch > 0 else { return nil }
        return TreeSealKey(publicKey: sealPublicKey, epoch: UInt32(sealEpoch))
    }
}
