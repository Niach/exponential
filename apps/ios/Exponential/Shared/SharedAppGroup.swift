import Foundation

/// Identifiers shared between the main app and the Share Extension. Prod and
/// staging use distinct groups so the two installs stay isolated (mirroring the
/// fact that they are separate apps). The values here must match the
/// `com.apple.security.application-groups` and `keychain-access-groups` entries
/// in the corresponding `.entitlements` file.
enum SharedAppGroup {
    /// Apple Developer team prefix (`$(AppIdentifierPrefix)` minus the trailing
    /// dot). Hardcoded to match `DEVELOPMENT_TEAM` in `Project.swift`; the system
    /// prepends `"<team>."` to keychain-access-groups, and a `SecItem` query must
    /// pass the full `"<team>.<suffix>"` string.
    static let teamPrefix = "V6W7BVCSM8"

    #if STAGING
    static let suiteName = "group.com.straehhuber.exponential.staging"
    static let keychainAccessGroup = "\(teamPrefix).com.straehhuber.exponential.staging.shared"
    #else
    static let suiteName = "group.com.straehhuber.exponential"
    static let keychainAccessGroup = "\(teamPrefix).com.straehhuber.exponential.shared"
    #endif

    /// Shared defaults backed by the app-group container. Nil only if the
    /// entitlement is missing (e.g. an unsigned simulator build).
    static var defaults: UserDefaults? { UserDefaults(suiteName: suiteName) }
}
