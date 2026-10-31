import Foundation

/// A-16: the Devices page row — which group a device sits in, whether it says
/// "No account", and what its attention card lists. Pure, and byte-locked by
/// `packages/domain-contract/fixtures/devices-list.json` (`cases`), which
/// every mirror replays: the TS reference `domain-contract/src/devices-list.ts`
/// (`deviceListRow`), desktop `ui::machines` (`device_attention`), Android
/// `domain/DevicesList.kt`. The fixture's `$comment` holds the rules.
public enum DevicesList {
    /// The fixture's `copy`, byte-identical ×4.
    public enum Copy {
        public static let mine = "My devices"
        public static let team = "Team devices"
        public static let offline = "Offline"
        public static let noAccount = "No account"
        public static let checking = "Checking…"
        public static let addAccount = "Add account"
        public static let attention = "Needs attention"
        public static let updateAvailable = "Update available · v{version}"
        public static let updateQueued = "Update queued: restarts within 10 minutes."
        public static let needsRelogin = "{label} · Needs re-login"
        public static let signedOut = "{label} · Signed out"
        public static let update = "Update"
        public static let signIn = "Sign in"
    }

    /// One login PROFILE the device reported. An agent with no profile adds
    /// nothing (a CLI installed but never signed in is not an item).
    public struct Login: Equatable, Sendable {
        public let agent: String
        public let profileId: String
        /// `AgentAccountsRows.loginLabel` — the email, else the plan, else
        /// the fallback.
        public let label: String
        public let health: AgentAccountHealth

        public init(agent: String, profileId: String, label: String, health: AgentAccountHealth) {
            self.agent = agent
            self.profileId = profileId
            self.label = label
            self.health = health
        }

        /// A login sub-row, as the Devices page derives it.
        public init(_ row: AgentProfileUsageRow) {
            self.init(
                agent: row.agent,
                profileId: row.profileId,
                label: AgentAccountsRows.loginLabel(row),
                health: row.health
            )
        }
    }

    public struct Input: Equatable, Sendable {
        public let online: Bool
        /// The caller's own machine (not a teammate's shared server).
        public let mine: Bool
        /// A headless CLI server: the only kind that acts on an update
        /// request (a desktop app updates itself).
        public let server: Bool
        public let registered: Bool
        /// The client's own version compare said the device is behind `latest`.
        public let outdated: Bool
        public let latest: String?
        public let updateQueued: Bool
        /// The machine sent its accounts at least once.
        public let accountsReported: Bool
        public let logins: [Login]

        public init(
            online: Bool,
            mine: Bool,
            server: Bool,
            registered: Bool,
            outdated: Bool,
            latest: String?,
            updateQueued: Bool,
            accountsReported: Bool,
            logins: [Login]
        ) {
            self.online = online
            self.mine = mine
            self.server = server
            self.registered = registered
            self.outdated = outdated
            self.latest = latest
            self.updateQueued = updateQueued
            self.accountsReported = accountsReported
            self.logins = logins
        }
    }

    public enum Group: String, Equatable, Sendable {
        case online
        case offline
    }

    public enum ItemKind: String, Equatable, Sendable {
        case login
        case update
        case updateQueued = "update_queued"
    }

    public enum ItemAction: String, Equatable, Sendable {
        case signIn = "sign_in"
        case update
    }

    /// One line of the attention card: `text` left, its one `action` pill
    /// right (nil = no pill).
    public struct AttentionItem: Equatable, Sendable, Identifiable {
        public let kind: ItemKind
        /// Login items only.
        public let agent: String?
        public let profileId: String?
        public let text: String
        public let action: ItemAction?

        public var id: String {
            "\(kind.rawValue):\(agent ?? ""):\(profileId ?? "")"
        }

        public init(
            kind: ItemKind,
            agent: String? = nil,
            profileId: String? = nil,
            text: String,
            action: ItemAction?
        ) {
            self.kind = kind
            self.agent = agent
            self.profileId = profileId
            self.text = text
            self.action = action
        }
    }

    public struct Row: Equatable, Sendable {
        public let group: Group
        public let noAccount: Bool
        public let attention: [AttentionItem]
    }

    /// `deviceListRow`: dead credentials first, then signed-out logins, then
    /// the update line (own devices only).
    public static func row(_ input: Input) -> Row {
        var attention: [AttentionItem] = []
        let canSignIn = input.mine && input.online
        let templates: [(AgentAccountHealth, String)] = [
            (.needsRelogin, Copy.needsRelogin),
            (.signedOut, Copy.signedOut),
        ]
        for (health, template) in templates {
            for login in input.logins where login.health == health {
                attention.append(AttentionItem(
                    kind: .login,
                    agent: login.agent,
                    profileId: login.profileId,
                    text: template.replacingOccurrences(of: "{label}", with: login.label),
                    action: canSignIn ? .signIn : nil
                ))
            }
        }
        if input.mine {
            if input.updateQueued {
                attention.append(AttentionItem(
                    kind: .updateQueued, text: Copy.updateQueued, action: nil
                ))
            } else if input.outdated, let latest = input.latest, !latest.isEmpty {
                attention.append(AttentionItem(
                    kind: .update,
                    text: Copy.updateAvailable.replacingOccurrences(of: "{version}", with: latest),
                    action: input.server && input.online && input.registered ? .update : nil
                ))
            }
        }
        return Row(
            group: input.online ? .online : .offline,
            noAccount: input.accountsReported && input.logins.isEmpty,
            attention: attention
        )
    }
}
