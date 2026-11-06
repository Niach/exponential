import Foundation
import GRDB

/// #EXP-1641: ONE message read off the `messages` shape proxy on demand (an
/// Electric subset snapshot, `subset__*` over the shape's own columns, the
/// same member-only visibility; Android `MessageSnapshot`), for a
/// notification action that answers a card this install has not synced yet.
/// The row lands in the local store raw (sealed as synced), exactly as the
/// shape loop writes it; the shape loop's own cursor is untouched.
public struct MessageSnapshot: Sendable {
    static let path = "/api/shapes/messages"

    private let auth: AuthRepository
    private let db: DatabaseManager
    private let session: URLSession

    public init(auth: AuthRepository, db: DatabaseManager, session: URLSession = .shared) {
        self.auth = auth
        self.db = db
        self.session = session
    }

    /// The message stored locally, nil when it is not visible. Throws on a
    /// failed read.
    @discardableResult
    public func fetch(accountId: String, messageId: String) async throws -> MessageEntity? {
        guard let account = auth.accounts.first(where: { $0.id == accountId }), let token = account.token,
              var components = URLComponents(string: account.instanceUrl.trimmingCharacters(in: CharacterSet(charactersIn: "/")) + Self.path)
        else { return nil }
        let params = try JSONSerialization.data(withJSONObject: ["1": messageId])
        components.queryItems = [
            URLQueryItem(name: "offset", value: "-1"),
            URLQueryItem(name: "log", value: "changes_only"),
            URLQueryItem(name: "subset__where", value: #""id" = $1"#),
            URLQueryItem(name: "subset__params", value: String(decoding: params, as: UTF8.self)),
            URLQueryItem(name: "subset__limit", value: "1"),
        ]
        guard let url = components.url else { return nil }
        var request = URLRequest(url: url)
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue(AppConstants.clientVersionHeaderValue, forHTTPHeaderField: "x-client-version")
        request.cachePolicy = .reloadIgnoringLocalAndRemoteCacheData
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            throw URLError(.badServerResponse)
        }
        guard let row = try Self.rows(data).first(where: { $0.id == messageId }) else { return nil }
        let pool = try db.pool(forAccountId: accountId)
        try await pool.write { try row.save($0) }
        return row
    }

    /// The inserts of a subset snapshot body (`{metadata, data: [...]}`, or a
    /// bare message array).
    static func rows(_ body: Data) throws -> [MessageEntity] {
        let parsed = try JSONSerialization.jsonObject(with: body)
        let items: [Any]
        if let object = parsed as? [String: Any], let data = object["data"] as? [Any] {
            items = data
        } else if let array = parsed as? [Any] {
            items = array
        } else {
            throw URLError(.cannotParseResponse)
        }
        return items.compactMap { item in
            guard let message = item as? [String: Any] else { return nil }
            let operation = (message["headers"] as? [String: Any])?["operation"] as? String
            if let operation, operation != "insert" { return nil }
            guard let value = message["value"] as? [String: Any],
                  let json = try? JSONSerialization.data(withJSONObject: value) else { return nil }
            return try? JSONDecoder().decode(MessageEntity.self, from: json)
        }
    }
}
