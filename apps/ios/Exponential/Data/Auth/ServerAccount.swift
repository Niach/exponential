import CryptoKit
import Foundation

struct ServerAccount: Codable, Identifiable, Equatable, Hashable, Sendable {
    var id: String
    var instanceUrl: String
    var token: String?
    var userEmail: String?
    var userName: String?
    var userId: String?
    var isAdmin: Bool
    var lastUsedAt: Date

    static func makeId(for instanceUrl: String) -> String {
        let digest = SHA256.hash(data: Data(instanceUrl.utf8))
        return digest.prefix(4).map { String(format: "%02x", $0) }.joined()
    }

    var displayHost: String {
        URL(string: instanceUrl)?.host ?? instanceUrl
    }
}
