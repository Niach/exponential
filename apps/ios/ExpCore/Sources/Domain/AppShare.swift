import Foundation

// VAPP-241 (packages/domain-contract/fixtures/app-share.json, spec
// docs/spec/apps.md §9.4): an app's PUBLIC LINKS. Mirrors the TS reference
// packages/domain-contract/src/app-share.ts; AppShareTests replays the
// fixture's `stateCases`, `capCases`, `bylineCases`, `rowCases` and
// `historyCases`. The guest's page and form are web-only.
public enum AppShare {
    /// Fixture `copy` (what the app clients show).
    public enum Copy {
        public static let title = "Public links"
        public static let submit = "Form"
        public static let view = "View"
        public static let off = "Off"
        public static let on = "On"
        public static let expired = "Expired"
        public static let orphaned = "No longer in the app"
        /// `{date}` = the link's expiry.
        public static let expiresOn = "until {date}"
        public static let copyLink = "Copy link"
        public static let newLink = "New link"
        public static let guest = "Guest"
    }

    /// Fixture `caps`: what `underShareCaps` reads.
    public enum Caps {
        public static let perIpPerHour = 10
        public static let perLinkPerHour = 200
    }

    /// Fixture `testIds`.
    public enum TestIds {
        public static let shareLinks = "app-settings-share-links"
        public static let shareRow = "app-settings-share-row"
    }

    /// The history `via` prefix of a guest's submission (`share:<name>`).
    public static let viaPrefix = "share:"

    /// A share's mode (TS `ShareMode`).
    public enum Mode: String, Sendable {
        case submit, view

        public var label: String {
            switch self {
            case .submit: return Copy.submit
            case .view: return Copy.view
            }
        }
    }

    /// A link's state (TS `ShareLinkState`).
    public enum LinkState: String, Sendable {
        case off, on, expired, orphaned

        public var label: String {
            switch self {
            case .off: return Copy.off
            case .on: return Copy.on
            case .expired: return Copy.expired
            case .orphaned: return Copy.orphaned
            }
        }
    }

    /// The part of an `app_share_links` row `linkState` reads.
    public struct Link: Equatable, Sendable {
        public let revokedAt: String?
        public let expiresAt: String?

        public init(revokedAt: String?, expiresAt: String?) {
            self.revokedAt = revokedAt
            self.expiresAt = expiresAt
        }
    }

    /// A link's state at `now` (`stateCases`); `declared` = the published
    /// package still declares the share.
    public static func linkState(_ link: Link?, declared: Bool, now: String) -> LinkState {
        guard let link, link.revokedAt == nil else { return .off }
        if !declared { return .orphaned }
        if let raw = link.expiresAt,
           let expires = WireTimestamps.parse(raw),
           let current = WireTimestamps.parse(now),
           expires <= current {
            return .expired
        }
        return .on
    }

    /// May one more submission land (`capCases`)?
    public static func underShareCaps(ipCount: Int, linkCount: Int) -> Bool {
        ipCount < Caps.perIpPerHour && linkCount < Caps.perLinkPerHour
    }

    /// The By/author label (`bylineCases`): a shared record reads Guest.
    public static func shareByline(_ shareName: String?, byline: String) -> String {
        guard let shareName, !shareName.isEmpty else { return byline }
        return Copy.guest
    }

    /// A Settings row's subtitle (`rowCases`).
    /// A Settings row's Switch (`switchCases`): an orphaned link still reads
    /// on, so its owner can revoke it.
    public static func shareSwitchOn(_ state: LinkState) -> Bool {
        state == .on || state == .orphaned
    }

    public static func shareRowSubtitle(_ mode: Mode, _ state: LinkState, expiresAt: String?) -> String {
        var parts = [mode.label, state.label]
        if state == .on, let expiresAt, !expiresAt.isEmpty {
            parts.append(Copy.expiresOn.replacingOccurrences(of: "{date}", with: expiresAt))
        }
        return parts.joined(separator: " · ")
    }

    /// Does a history event's `via` read Guest as its actor (`historyCases`)?
    public static func isGuestVia(_ via: String?) -> Bool {
        via?.hasPrefix(viaPrefix) == true
    }
}
