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
    public var lastUsedAt: Date

    public init(
        id: String,
        instanceUrl: String,
        token: String?,
        userEmail: String?,
        userName: String?,
        userId: String?,
        isAdmin: Bool,
        lastUsedAt: Date
    ) {
        self.id = id
        self.instanceUrl = instanceUrl
        self.token = token
        self.userEmail = userEmail
        self.userName = userName
        self.userId = userId
        self.isAdmin = isAdmin
        self.lastUsedAt = lastUsedAt
    }

    public static func makeId(for instanceUrl: String) -> String {
        let digest = SHA256.hash(data: Data(instanceUrl.utf8))
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
