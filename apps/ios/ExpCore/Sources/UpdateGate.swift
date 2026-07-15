import Foundation

/// The single source of truth for the client-version gate (EXP-104). When the
/// server answers any request with HTTP 426 (`client_upgrade_required`) — the
/// build is below the configured minimum — the HTTP/shape layers trip this gate
/// and the SwiftUI root swaps in the blocking Update-required view. Mirrors the
/// `SyncDebug.shared` singleton-observable pattern (main-actor mutation hop,
/// `@unchecked Sendable` so the sync loops can call it from any context).
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

    /// Non-nil once the server has rejected this client version. Set ONCE — the
    /// first trigger wins — and never cleared for the process lifetime: an
    /// out-of-date build can't become current without relaunching (with an
    /// updated binary). Read from the SwiftUI root to gate the whole app.
    public private(set) var upgrade: UpgradeInfo?

    private init() {}

    /// Record that the server rejected this client version. Hops to the main
    /// actor (observation state must mutate there) and keeps the FIRST payload —
    /// concurrent shape/tRPC 426s all race here on app wake.
    public func trigger(min: String?, latest: String?) {
        Task { @MainActor in
            guard self.upgrade == nil else { return }
            self.upgrade = UpgradeInfo(min: min, latest: latest)
        }
    }
}

/// The 426 `client_upgrade_required` response body. Every field is optional —
/// the gate only needs min/latest and must NEVER throw on a partial or absent
/// payload (decoded via `try?` at both call sites).
struct ClientUpgradeResponse: Decodable {
    let min: String?
    let latest: String?
}
