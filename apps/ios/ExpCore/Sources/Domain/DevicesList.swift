import Foundation

/// A-16 / EXP-1560: the Devices page card — which group a device sits in,
/// whether it says "No account", which logins wear the attention badge (and
/// what their panel says), which account's panel is open, the update line its `⋯` menu leads with, and how
/// many account icons show. Pure, and byte-locked by
/// `packages/domain-contract/fixtures/devices-list.json` (`cases`), which
/// every mirror replays (`cases`, `accountIcons`, `accountPanelCases`): the TS reference
/// `domain-contract/src/devices-list.ts` (`deviceListRow`), desktop `ui::machines` (`device_attention`), Android
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
        public static let updateAvailable = "Update available · v{version}"
        public static let updateQueued = "Update queued: restarts within 10 minutes."
        public static let needsRelogin = "{label} · Needs re-login"
        public static let signedOut = "{label} · Signed out"
        public static let update = "Update"
        public static let signIn = "Sign in"
        /// EXP-1627: the sign-in dialog's title and its ONE muted waiting
        /// line ({device} = the device label, {agent} = the agent label).
        public static let signInTitle = "Sign in"
        public static let signInWaiting = "Waiting for {device} to publish the {agent} sign-in link…"
        public static let limitReached = "{label} · Limit reached"
        public static let deviceActions = "Device actions"
        public static let settings = "Device settings"
        public static let removeFromTeam = "Remove from this team"
        public static let moreAccounts = "More accounts"
        public static let longLivedRefused = "A long-lived login needs a Claude account on this machine."
        /// EXP-1626 PERSIST LOGIN: the account panel's option on an existing
        /// signed-in claude login (`persistLogin` without a token,
        /// `renewPersistedLogin` with one; the account panel only, no renewal
        /// reminder), the sign-in sheet's one muted line in that mode, and
        /// the panel's persisted line.
        public static let persistLogin = "Persist login"
        public static let renewPersistedLogin = "Renew persisted login"
        public static let persistNote = "Persist login keeps this account's runs signed in for 1 year, even if its browser login expires."
        public static let persistedBrowser = "Persisted · usage from the browser login"
        public static let persistedToken = "Persisted · browser login expired, weekly usage only"

        /// `copy.status`: the account panel's warning-tone status line.
        public static func status(_ status: LoginStatus) -> String {
            switch status {
            case .needsRelogin: "Needs re-login"
            case .signedOut: "Signed out"
            case .limit: "Limit reached"
            }
        }
    }

    /// EXP-1560: the card grid's geometry (`fixture.card`), ×4.
    public enum Card {
        public static let minWidth: CGFloat = 256
        public static let gap: CGFloat = 12
        public static let padding: CGFloat = 12
        public static let iconSize: CGFloat = 28
        public static let markSize: CGFloat = 16
        public static let maxAccountIcons = 5
        /// #EXP-1643: the device's own icon leading the card's top row.
        public static let deviceIconSize: CGFloat = 20
        /// #EXP-1643: above and below the account panel's hairline.
        public static let panelGap: CGFloat = 12
    }

    /// An ISO-8601 stamp, with or without fractional seconds.
    public static func parseIso(_ value: String) -> Date? {
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractional.date(from: value) ?? ISO8601DateFormatter().date(from: value)
    }

    /// EXP-1560 `deviceCardColumns`: grid columns for a section `width` wide.
    public static func columns(width: CGFloat) -> Int {
        max(1, Int(((width + Card.gap) / (Card.minWidth + Card.gap)).rounded(.down)))
    }

    /// EXP-1560 `deviceCardAccountIcons`: how many account icons a card
    /// shows, and whether the `⋯` "More accounts" button follows them.
    public struct AccountIcons: Equatable, Sendable {
        public let visible: Int
        public let more: Bool
    }

    public static func accountIcons(count: Int, expanded: Bool) -> AccountIcons {
        if expanded || count <= Card.maxAccountIcons {
            return AccountIcons(visible: count, more: false)
        }
        return AccountIcons(visible: Card.maxAccountIcons - 1, more: true)
    }

    /// #EXP-1643 `openDeviceAccount` (`accountPanelCases`): the login a card's
    /// panel shows. `remembered`, per device on this client: `.none` = this
    /// device was never touched here, `.some(nil)` = closed by hand, a key =
    /// the login opened last. A remembered login that is gone counts as
    /// untouched: the only login when there is exactly one, else nothing.
    public static func openDeviceAccount(loginKeys: [String], remembered: String??) -> String? {
        if case .some(.none) = remembered { return nil }
        if case let .some(.some(key)) = remembered, loginKeys.contains(key) { return key }
        return loginKeys.count == 1 ? loginKeys[0] : nil
    }

    /// EXP-1560 `limitHit`: a FRESH usage window at ≥ 100% (fresh = not
    /// flagged stale and fetched within `AgentUsagePresentation.freshWindow`).
    public static func limitHit(_ usage: AgentUsage?, now: Date = Date()) -> Bool {
        guard let usage, usage.stale != true,
              AgentUsagePresentation.isFresh(fetchedAt: usage.fetchedAt, now: now) else { return false }
        return (usage.windows ?? []).contains { ($0.percent ?? 0) >= 100 }
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
        /// EXP-1560: a fresh usage window at ≥ 100% (`limitHit(_:now:)`).
        public let limitHit: Bool
        /// EXP-1600: when the profile's long-lived claude login ends (ISO);
        /// nil = it holds none.
        public let tokenExpiresAt: String?

        public init(
            agent: String,
            profileId: String,
            label: String,
            health: AgentAccountHealth,
            limitHit: Bool = false,
            tokenExpiresAt: String? = nil
        ) {
            self.agent = agent
            self.profileId = profileId
            self.label = label
            self.health = health
            self.limitHit = limitHit
            self.tokenExpiresAt = tokenExpiresAt
        }

        /// A login sub-row, as the Devices page derives it.
        public init(_ row: AgentProfileUsageRow, now: Date = Date()) {
            self.init(
                agent: row.agent,
                profileId: row.profileId,
                label: AgentAccountsRows.loginLabel(row),
                health: row.health,
                limitHit: DevicesList.limitHit(row.usage, now: now),
                tokenExpiresAt: row.tokenExpiresAt
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
        /// EXP-1600: the clock the renewal reminder reads.
        public let now: Date

        public init(
            online: Bool,
            mine: Bool,
            server: Bool,
            registered: Bool,
            outdated: Bool,
            latest: String?,
            updateQueued: Bool,
            accountsReported: Bool,
            logins: [Login],
            now: Date = Date()
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
            self.now = now
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

    /// EXP-1560: a login item's panel status line (`copy.status`).
    public enum LoginStatus: String, Equatable, Sendable {
        case needsRelogin = "needs_relogin"
        case signedOut = "signed_out"
        case limit
    }

    public enum ItemAction: String, Equatable, Sendable {
        case signIn = "sign_in"
        case update
    }

    /// One attention item: a login's badge (+ its panel's status line) or
    /// the update line the card's `⋯` menu leads with.
    public struct AttentionItem: Equatable, Sendable, Identifiable {
        public let kind: ItemKind
        /// Login items only.
        public let agent: String?
        public let profileId: String?
        public let text: String
        public let action: ItemAction?
        /// Login items only (EXP-1560).
        public let status: LoginStatus?

        public var id: String {
            "\(kind.rawValue):\(agent ?? ""):\(profileId ?? "")"
        }

        public init(
            kind: ItemKind,
            agent: String? = nil,
            profileId: String? = nil,
            text: String,
            action: ItemAction?,
            status: LoginStatus? = nil
        ) {
            self.kind = kind
            self.agent = agent
            self.profileId = profileId
            self.text = text
            self.action = action
            self.status = status
        }
    }

    public struct Row: Equatable, Sendable {
        public let group: Group
        public let noAccount: Bool
        public let attention: [AttentionItem]
    }

    /// `deviceListRow`: dead credentials first, then signed-out logins, then
    /// every other login at its limit (never an action), then the update line
    /// (own devices only). EXP-1626: a persisted login is never an item (no
    /// renewal reminder).
    public static func row(_ input: Input) -> Row {
        var attention: [AttentionItem] = []
        let canSignIn = input.mine && input.online
        let templates: [(AgentAccountHealth, String, LoginStatus)] = [
            (.needsRelogin, Copy.needsRelogin, .needsRelogin),
            (.signedOut, Copy.signedOut, .signedOut),
        ]
        for (health, template, status) in templates {
            for login in input.logins where login.health == health {
                attention.append(AttentionItem(
                    kind: .login,
                    agent: login.agent,
                    profileId: login.profileId,
                    text: template.replacingOccurrences(of: "{label}", with: login.label),
                    action: canSignIn ? .signIn : nil,
                    status: status
                ))
            }
        }
        for login in input.logins
        where login.limitHit && login.health != .needsRelogin && login.health != .signedOut {
            attention.append(AttentionItem(
                kind: .login,
                agent: login.agent,
                profileId: login.profileId,
                text: Copy.limitReached.replacingOccurrences(of: "{label}", with: login.label),
                action: nil,
                status: .limit
            ))
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
