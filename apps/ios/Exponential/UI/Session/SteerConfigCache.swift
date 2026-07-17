import ExpCore
import SwiftUI

/// steer.config is env-derived and static per instance — fetch once per
/// account and cache for the app's lifetime (mirrors the web's fetch-once).
///
/// EXP-156: only a SUCCESSFUL fetch is memoized. A transient failure (offline,
/// server blip) falls back for that single call without poisoning the cache —
/// otherwise one early failure would pin steering off for the whole app
/// lifetime, and every `.task` re-runs on appear so the next visit retries.
@MainActor
enum SteerConfigCache {
    private static var cache: [String: SteerConfig] = [:]

    static func load(accountId: String, api: SteerApi) async -> SteerConfig {
        if let cached = cache[accountId] { return cached }
        guard let config = try? await api.config(accountId: accountId) else {
            // Transient failure: fall back for THIS call only — a memoized
            // failure would pin steering off for the whole app lifetime.
            return SteerConfig(enabled: false, relayUrl: nil)
        }
        cache[accountId] = config
        return config
    }
}
