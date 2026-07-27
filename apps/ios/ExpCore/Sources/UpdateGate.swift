import Foundation

/// The client-version gate (EXP-104), keyed PER ACCOUNT. When a server answers a
/// request with HTTP 426 (`client_upgrade_required`) — this build is below THAT
/// instance's configured minimum — the HTTP/shape layers trip the gate for the
/// account that made the request, and only that account stops: every other
/// signed-in server keeps syncing, and the SwiftUI root swaps in the blocking
/// Update-required view only while the gated account is the ACTIVE one (REV2-43;
/// a process-global gate let one outdated or misconfigured instance brick the
/// whole multi-account app with no escape). Mirrors the `SyncDebug.shared`
/// singleton-observable pattern (main-actor mutation, `@unchecked Sendable` so
/// the sync loops can reach it from any context).
@Observable
public final class UpdateGate: @unchecked Sendable {
    public static let shared = UpdateGate()

    public struct UpgradeInfo: Sendable, Equatable {
        /// Minimum supported version the server reported (may be absent).
        public let min: String?
        /// Latest available version the server reported (may be absent/null).
        public let latest: String?

        public init(min: String?, latest: String?) {
            self.min = min
            self.latest = latest
        }
    }

    /// Gated accounts → what their server reported. An account's entry is set
    /// ONCE — the first trigger wins, since concurrent shape/tRPC 426s all race
    /// here on app wake — and is never cleared while the account exists: an
    /// out-of-date build can't become current without relaunching with an
    /// updated binary. Mutated on the main actor, read from view bodies.
    public private(set) var upgrades: [String: UpgradeInfo] = [:]

    private init() {}

    /// What one account's server reported, or nil when it isn't gated. A nil
    /// accountId (no active account yet) is never gated.
    public func upgrade(forAccountId accountId: String?) -> UpgradeInfo? {
        guard let accountId else { return nil }
        return upgrades[accountId]
    }

    /// Every account whose server rejected this build. The nav layer blocks only
    /// the active one and badges the rest.
    public var gatedAccountIds: Set<String> { Set(upgrades.keys) }

    /// Record that `accountId`'s server rejected this client version.
    @MainActor
    public func trigger(accountId: String, min: String?, latest: String?) {
        guard upgrades[accountId] == nil else { return }
        upgrades[accountId] = UpgradeInfo(min: min, latest: latest)
    }

    /// Forget an account's gate — called when the account leaves the device (the
    /// blocking view's "Remove this server" escape), so re-adding that server on
    /// a supported build starts clean.
    @MainActor
    public func clear(accountId: String) {
        upgrades.removeValue(forKey: accountId)
    }
}

/// The 426 `client_upgrade_required` response body. Every field is optional —
/// the gate only needs min/latest and must NEVER throw on a partial or absent
/// payload (decoded via `try?` at both call sites).
struct ClientUpgradeResponse: Decodable {
    let min: String?
    let latest: String?
}
