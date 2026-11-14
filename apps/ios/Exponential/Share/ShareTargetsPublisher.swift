import ExpCore
import ExpUI
import Foundation
import GRDB
import Intents
import OSLog
import SwiftUI
import UIKit

private let logger = Logger(subsystem: "at.exponential", category: "ShareTargets")

/// EXP-1635: keeps what the share extension reads current: one observation
/// per signed-in account over the agents, the root sessions and the devices,
/// throttled (a session row moves on every turn), writes
/// `SharedShareMirror`, and for the ACTIVE account donates the system share
/// sheet's rows (`DirectShareDonations`) and the app icon's long-press menu
/// (#EXP-1638, `AppShortcutItems`).
@MainActor
final class ShareTargetsPublisher {
    /// One pass per account at most this often.
    private static let throttle: Duration = .seconds(5)
    /// Recent chats mirrored: more than the sheet's cap, so a chat that
    /// leaves the donation still resolves.
    nonisolated private static let chatLimit = 12

    private let auth: AuthRepository
    private let db: DatabaseManager
    private var tasks: [String: Task<Void, Never>] = [:]
    private var donated: [String: [DirectShareDonations.Row]] = [:]
    private var shortcuts: [String: [AppShortcuts.Entry]] = [:]

    init(auth: AuthRepository, db: DatabaseManager) {
        self.auth = auth
        self.db = db
    }

    /// Reconcile with `auth.accounts` (call when they may have changed).
    func refresh() {
        let signedIn = auth.accounts.filter { $0.token != nil }
        let desired = Set(signedIn.map(\.id))
        for (accountId, task) in tasks where !desired.contains(accountId) {
            task.cancel()
            tasks[accountId] = nil
            donated[accountId] = nil
            shortcuts[accountId] = nil
        }
        for account in signedIn where tasks[account.id] == nil {
            start(accountId: account.id, userId: account.userId)
        }
        donate()
    }

    private func start(accountId: String, userId: String?) {
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try Self.read(db, userId: userId)
        }.removeDuplicates()
        tasks[accountId] = Task { @MainActor [weak self] in
            do {
                let values = observation.values(in: pool, bufferingPolicy: .bufferingNewest(1))
                for try await snapshot in values {
                    guard let self else { return }
                    SharedShareMirror.write(accountId: accountId, snapshot.targets)
                    self.donated[accountId] = snapshot.rows
                    self.shortcuts[accountId] = snapshot.shortcuts
                    self.donate()
                    try await Task.sleep(for: Self.throttle)
                }
            } catch is CancellationError {
            } catch {
                logger.error("Share targets stream ended: \(error.localizedDescription)")
            }
        }
    }

    /// The active account's rows, once its first read landed (a launch must
    /// not wipe the donations before it); signed out = none.
    /// #EXP-1638: the app shortcuts follow the ACTIVE account the same way.
    private func donate() {
        guard auth.hasAuthenticatedAccount else {
            AppShortcutItems.publish([])
            return DirectShareDonations.donate([])
        }
        if let entries = auth.activeAccountId.flatMap({ shortcuts[$0] }) {
            AppShortcutItems.publish(entries)
        }
        guard let rows = auth.activeAccountId.flatMap({ donated[$0] }) else { return }
        DirectShareDonations.donate(rows)
    }

    /// One account's mirror plus its donation rows.
    private struct Snapshot: Equatable, Sendable {
        let targets: MirroredShareTargets
        let rows: [DirectShareDonations.Row]
        /// #EXP-1638 / #EXP-1830: the long-press menu (statics + chat entries).
        let shortcuts: [AppShortcuts.Entry]
    }

    nonisolated private static func read(_ db: Database, userId: String?) throws -> Snapshot {
        let definitions = try AgentDefinitionEntity.fetchAll(db)
        let conversations: [SessionEntity] = userId == nil ? [] : try SessionEntity.fetchAll(db, sql: """
            SELECT * FROM "sessions" WHERE "agent_id" IS NOT NULL AND "user_id" = ? AND "parent_id" IS NULL
            ORDER BY "created_at" ASC
            """, arguments: [userId])
        // Later rows win: the newest conversation, as `SessionSealer` reads it.
        let conversationByAgent = Dictionary(
            conversations.compactMap { row in row.agentId.map { ($0, row) } },
            uniquingKeysWith: { _, new in new }
        )
        let agents = definitions.map { agent -> MirroredAgent in
            MirroredAgent(
                id: agent.id,
                teamId: agent.teamId,
                name: agent.name,
                glyph: BoardTypeDisplay.iconName(for: agent.icon) ?? SessionTree.ListCopy.agentDefaultIcon,
                color: agent.color ?? SessionTree.ListCopy.agentDefaultColor,
                sortOrder: agent.sortOrder,
                role: agent.role,
                ownerId: agent.ownerId,
                conversation: conversationByAgent[agent.id].map(sealRoot)
            )
        }
        let roots = try SessionEntity.fetchAll(db, sql: """
            SELECT * FROM "sessions"
            WHERE "parent_id" IS NULL AND "agent_id" IS NULL AND "app_id" IS NULL
              AND "seal_public_key" IS NULL AND "sealed_title" IS NULL
            ORDER BY "updated_at" DESC LIMIT ?
            """, arguments: [chatLimit])
        let chats = roots
            .filter { userId == nil || $0.userId == userId || $0.shared }
            .map { root in
                MirroredChat(
                    title: root.openedTitle ?? ShareTargets.Copy.newChat,
                    updatedAt: root.updatedAt,
                    root: sealRoot(root)
                )
            }
        let deviceRows = try DeviceEntity.fetchAll(db)
        let users = try UserEntity.fetchAll(db)
        let teamIds = Set(try TeamEntity.fetchAll(db).map(\.id))
        let rowIds = Dictionary(grouping: deviceRows, by: \.deviceId).mapValues { $0.map(\.id) }
        let devices = DeviceQueries.compose(rows: deviceRows, users: users, teamIds: teamIds, userId: userId)
            .filter(\.hasRunnableAgent)
            .map { device in
                MirroredDevice(
                    deviceId: device.deviceId,
                    rowIds: rowIds[device.deviceId] ?? [],
                    label: LaunchVocabulary.deviceName(device),
                    glyph: DeviceIconDisplay.iconName(for: device),
                    online: device.isOnline,
                    isDefault: device.isDefaultDevice,
                    isMine: device.owner == nil,
                    sharedTeamIds: device.sharedTeamIds
                )
            }
        let targets = MirroredShareTargets(userId: userId, agents: agents, chats: chats, devices: devices)
        let entries = ShareTargets.directShareEntries(
            agents: agents,
            chats: roots.map { root in
                ShareTargets.Chat(
                    id: root.id,
                    title: root.openedTitle ?? ShareTargets.Copy.newChat,
                    updatedAt: root.updatedAt,
                    agentId: root.agentId
                )
            },
            me: userId
        )
        let agentById = Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0) })
        let rows = entries.map { entry -> DirectShareDonations.Row in
            guard case let .agent(id)? = ShareTargets.parseDirectShareId(entry.id), let agent = agentById[id],
                  !AgentOrdering.isAssistant(role: agent.role) else {
                return DirectShareDonations.Row(id: entry.id, title: entry.title, glyph: nil, color: nil)
            }
            return DirectShareDonations.Row(id: entry.id, title: entry.title, glyph: agent.glyph, color: agent.color)
        }
        // #EXP-1638 / #EXP-1830 (app-shortcuts.json): the Assistant, the
        // session pins, then the recent chats; an own agent conversation is
        // its agent's entry, a sealed plain chat never reaches the OS (the
        // roots query already drops them; `sealed` restates it).
        let visible = definitions.filter { AgentOrdering.visibleAgent(role: $0.role, ownerId: $0.ownerId, me: userId) }
        let assistant = visible.first { AgentOrdering.isAssistant(role: $0.role) && $0.ownerId == userId }
        let pins: [String] = userId == nil ? [] : try String.fetchAll(db, sql: """
            SELECT "session_id" FROM "pins"
            WHERE "kind" = 'session' AND "user_id" = ? AND "session_id" IS NOT NULL
            ORDER BY COALESCE("sort_order", 0) ASC, "created_at" ASC
            """, arguments: [userId])
        let listed = Set((conversations + roots).map(\.id))
        let pinnedRoots = try SessionEntity.fetchAll(db, keys: pins.filter { !listed.contains($0) })
            .filter { $0.parentId == nil }
        let shortcuts = AppShortcuts.appShortcuts(AppShortcuts.Input(
            assistant: assistant.map { AppShortcuts.Assistant(id: $0.id, name: $0.name) },
            agents: visible.map { agent in
                AppShortcuts.Agent(
                    id: agent.id, name: agent.name, icon: BoardTypeDisplay.iconName(for: agent.icon), color: agent.color
                )
            },
            pins: pins,
            chats: (conversations + roots + pinnedRoots).map { root in
                AppShortcuts.Chat(
                    id: root.id,
                    title: root.openedTitle ?? ShareTargets.Copy.newChat,
                    updatedAt: root.updatedAt,
                    agentId: root.agentId,
                    sealed: root.sealKey != nil || root.sealedTitle != nil
                )
            }
        ))
        return Snapshot(targets: targets, rows: rows, shortcuts: shortcuts)
    }

    nonisolated private static func sealRoot(_ root: SessionEntity) -> MirroredSealRoot {
        MirroredSealRoot(
            id: root.rootId,
            teamId: root.teamId,
            deviceId: root.deviceId,
            shared: root.shared,
            publicKey: root.sealKey?.publicKey,
            epoch: root.sealKey?.epoch
        )
    }
}

/// `INSendMessageIntent` donations: what puts agents and chats into the
/// system share sheet's top row. The share extension reads the
/// `conversationIdentifier` back (`ShareTargets.parseDirectShareId`).
enum DirectShareDonations {
    struct Row: Equatable, Sendable {
        let id: String
        let title: String
        /// An agent's registry glyph and colour; nil = no image (the
        /// Assistant's brand mark and a chat wear the app's own).
        let glyph: String?
        let color: String?
    }

    private static let groupIdentifier = "exp-direct-share"
    private static let signatureKey = "direct_share_signature_v1"

    /// Replaces the donations; a no-op while the rows stand (the system ranks
    /// by recency, so the first row is donated last).
    @MainActor
    static func donate(_ rows: [Row]) {
        let signature = rows.map { "\($0.id)\u{1}\($0.title)\u{1}\($0.glyph ?? "")\u{1}\($0.color ?? "")" }
            .joined(separator: "\u{2}")
        let defaults = UserDefaults.standard
        guard defaults.string(forKey: signatureKey) != signature else { return }
        defaults.set(signature, forKey: signatureKey)
        let interactions = rows.reversed().map(interaction)
        INInteraction.delete(with: groupIdentifier) { _ in
            for interaction in interactions {
                interaction.donate { error in
                    if let error { logger.error("Share donation failed: \(error.localizedDescription)") }
                }
            }
        }
    }

    @MainActor
    private static func interaction(_ row: Row) -> INInteraction {
        let intent = INSendMessageIntent(
            recipients: nil,
            outgoingMessageType: .outgoingMessageText,
            content: nil,
            speakableGroupName: INSpeakableString(spokenPhrase: row.title),
            conversationIdentifier: row.id,
            serviceName: nil,
            sender: nil,
            attachments: nil
        )
        if let glyph = row.glyph, let data = iconData(glyph: glyph, color: row.color) {
            intent.setImage(INImage(imageData: data), forParameterNamed: \.speakableGroupName)
        }
        let interaction = INInteraction(intent: intent, response: nil)
        interaction.direction = .outgoing
        interaction.groupIdentifier = groupIdentifier
        return interaction
    }

    /// The agent's glyph on the bubble's muted disc.
    @MainActor
    private static func iconData(glyph: String, color: String?) -> Data? {
        let side: CGFloat = 120
        guard let icon = AppIcons.uiImage(glyph, pointSize: side / 2) else { return nil }
        let tint = color.flatMap { UIColor(Color(hex: $0) ?? .white) } ?? .white
        let image = UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { context in
            UIColor(DesignTokens.Palette.muted).setFill()
            context.cgContext.fillEllipse(in: CGRect(x: 0, y: 0, width: side, height: side))
            icon.withTintColor(tint, renderingMode: .alwaysOriginal)
                .draw(in: CGRect(x: side / 4, y: side / 4, width: side / 2, height: side / 2))
        }
        return image.pngData()
    }
}
