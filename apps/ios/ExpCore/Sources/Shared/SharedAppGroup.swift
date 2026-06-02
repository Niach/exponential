import Foundation

/// Identifiers shared between the main app and the Share Extension. Prod and
/// staging use distinct groups so the two installs stay isolated (mirroring the
/// fact that they are separate apps). The values here must match the
/// `com.apple.security.application-groups` and `keychain-access-groups` entries
/// in the corresponding `.entitlements` file.
public enum SharedAppGroup {
    /// Apple Developer team prefix (`$(AppIdentifierPrefix)` minus the trailing
    /// dot). Hardcoded to match `DEVELOPMENT_TEAM` in `Project.swift`; the system
    /// prepends `"<team>."` to keychain-access-groups, and a `SecItem` query must
    /// pass the full `"<team>.<suffix>"` string.
    public static let teamPrefix = "V6W7BVCSM8"

    /// Staging detected from the host bundle id (the staging app/extension bundles
    /// carry `.staging`), not a compile-time `#if STAGING` — so it resolves
    /// correctly from inside the once-compiled `ExpCore` framework regardless of
    /// which app links it.
    private static var isStaging: Bool {
        Bundle.main.bundleIdentifier?.contains(".staging") ?? false
    }

    public static var suiteName: String {
        isStaging
            ? "group.com.straehhuber.exponential.staging"
            : "group.com.straehhuber.exponential"
    }

    public static var keychainAccessGroup: String {
        isStaging
            ? "\(teamPrefix).com.straehhuber.exponential.staging.shared"
            : "\(teamPrefix).com.straehhuber.exponential.shared"
    }

    /// Shared defaults backed by the app-group container. Nil only if the
    /// entitlement is missing (e.g. an unsigned simulator build).
    public static var defaults: UserDefaults? { UserDefaults(suiteName: suiteName) }
}
