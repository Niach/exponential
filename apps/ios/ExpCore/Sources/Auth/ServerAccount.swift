import CryptoKit
import Foundation

public struct ServerAccount: Codable, Identifiable, Equatable, Hashable, Sendable {
    public var id: String
    public var instanceUrl: String
    public var token: String?
    public var userEmail: String?
    public var userName: String?
    public var userId: String?
    public var isAdmin: Bool
    // ISO timestamp the user finished onboarding, or nil if they haven't. Read
    // from the better-auth session at login (the same source the web app gates
    // on) and persisted so the onboarding gate resolves synchronously at startup.
    public var onboardingCompletedAt: String?
    // True once onboardingCompletedAt was actually read from the server. Optional
    // (not Bool) so accounts persisted by builds before the onboarding field
    // existed still decode — they come back nil and are treated as already
    // onboarded; only a session read that explicitly reported "not completed"
    // should start the wizard.
    public var onboardingKnown: Bool?
    public var lastUsedAt: Date

    public init(
        id: String,
        instanceUrl: String,
        token: String?,
        userEmail: String?,
        userName: String?,
        userId: String?,
        isAdmin: Bool,
        onboardingCompletedAt: String? = nil,
        onboardingKnown: Bool? = nil,
        lastUsedAt: Date
    ) {
        self.id = id
        self.instanceUrl = instanceUrl
        self.token = token
        self.userEmail = userEmail
        self.userName = userName
        self.userId = userId
        self.isAdmin = isAdmin
        self.onboardingCompletedAt = onboardingCompletedAt
        self.onboardingKnown = onboardingKnown
        self.lastUsedAt = lastUsedAt
    }

    // The nav gate: show the first-run wizard only when the server told us
    // onboarding isn't done. Legacy accounts (onboardingKnown == nil) never bounce.
    public var needsOnboarding: Bool {
        onboardingKnown == true && onboardingCompletedAt == nil
    }

    /// Pre-login ("pending") id — keyed by instance URL only, used before a
    /// session resolves the user. Two users on the same server would collide
    /// here; that's why a resolved login switches to the per-user id below.
    public static func makeId(for instanceUrl: String) -> String {
        let digest = SHA256.hash(data: Data(instanceUrl.utf8))
        return digest.prefix(4).map { String(format: "%02x", $0) }.joined()
    }

    /// Per-user account id — keyed by instance URL AND userId, so two users on
    /// the same server get distinct ids (hence distinct local DB files, offsets,
    /// and workspace selection). The `\n` separator keeps URL/userId boundaries
    /// unambiguous. Same 4-byte hex width as the pending id.
    public static func makeId(instanceUrl: String, userId: String) -> String {
        let digest = SHA256.hash(data: Data("\(instanceUrl)\n\(userId)".utf8))
        return digest.prefix(4).map { String(format: "%02x", $0) }.joined()
    }

    public var displayHost: String {
        URL(string: instanceUrl)?.host ?? instanceUrl
    }

    public var displayName: String {
        if instanceUrl == AppConstants.publicCloudUrl { return "Cloud" }
        if instanceUrl == AppConstants.stagingCloudUrl { return "Staging" }
        return displayHost
    }
}
