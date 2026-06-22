import Foundation

public final class HTTPClient: Sendable {
    private let auth: AuthRepository
    public let session: URLSession

    public init(auth: AuthRepository) {
        self.auth = auth
        let config = URLSessionConfiguration.default
        config.httpAdditionalHeaders = [
            "Accept": "application/json",
        ]
        config.timeoutIntervalForRequest = 30
        self.session = URLSession(configuration: config)
    }

    public func request(_ url: URL, accountId: String, method: String = "GET", body: Data? = nil, contentType: String? = "application/json") -> URLRequest {
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.httpBody = body
        if let contentType {
            request.setValue(contentType, forHTTPHeaderField: "Content-Type")
        }
        if let token = auth.accounts.first(where: { $0.id == accountId })?.token {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        return request
    }

    public func perform(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let (data, response) = try await session.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw HTTPError.invalidResponse
        }
        return (data, httpResponse)
    }

    public func get(_ url: URL, accountId: String) async throws -> (Data, HTTPURLResponse) {
        try await perform(request(url, accountId: accountId))
    }

    public func post(_ url: URL, accountId: String, body: Data) async throws -> (Data, HTTPURLResponse) {
        try await perform(request(url, accountId: accountId, method: "POST", body: body))
    }

    /// POST a single file as `multipart/form-data` under field name `file` (the
    /// shape the `/api/issues/{id}/images` route expects). Authed for `accountId`.
    /// Mirrors the hand-rolled boundary in IssueImagesApi so other callers (the
    /// preview feedback reporter) don't re-implement it.
    public func postMultipart(
        _ url: URL,
        accountId: String,
        fileData: Data,
        filename: String,
        contentType: String
    ) async throws -> (Data, HTTPURLResponse) {
        let boundary = "Boundary-\(UUID().uuidString)"
        var body = Data()
        body.append(Data("--\(boundary)\r\n".utf8))
        body.append(Data("Content-Disposition: form-data; name=\"file\"; filename=\"\(filename)\"\r\n".utf8))
        body.append(Data("Content-Type: \(contentType)\r\n\r\n".utf8))
        body.append(fileData)
        body.append(Data("\r\n--\(boundary)--\r\n".utf8))
        var req = request(
            url, accountId: accountId, method: "POST", body: body,
            contentType: "multipart/form-data; boundary=\(boundary)"
        )
        req.setValue("\(body.count)", forHTTPHeaderField: "Content-Length")
        return try await perform(req)
    }

    // GET with an explicit bearer token — used by AuthApi.fetchSession during
    // login, before the token is persisted to the account store.
    public func get(_ url: URL, bearerToken: String?) async throws -> (Data, HTTPURLResponse) {
        var req = URLRequest(url: url)
        req.httpMethod = "GET"
        if let bearerToken {
            req.setValue("Bearer \(bearerToken)", forHTTPHeaderField: "Authorization")
        }
        let (data, response) = try await session.data(for: req)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw HTTPError.invalidResponse
        }
        return (data, httpResponse)
    }

    // Unauthenticated GET — used by AuthApi.fetchAuthConfig before any
    // account exists.
    public func getUnauthenticated(_ url: URL) async throws -> (Data, HTTPURLResponse) {
        var req = URLRequest(url: url)
        req.httpMethod = "GET"
        let (data, response) = try await session.data(for: req)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw HTTPError.invalidResponse
        }
        return (data, httpResponse)
    }

    // Unauthenticated POST — used by AuthApi.signInWithPassword before
    // the token is stored.
    public func postUnauthenticated(_ url: URL, body: Data) async throws -> (Data, HTTPURLResponse) {
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.httpBody = body
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        let (data, response) = try await session.data(for: req)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw HTTPError.invalidResponse
        }
        return (data, httpResponse)
    }
}

public enum HTTPError: Error, LocalizedError, Sendable {
    case invalidResponse
    case httpError(Int, String)

    public var errorDescription: String? {
        switch self {
        case .invalidResponse: "Invalid server response"
        case let .httpError(code, message): "HTTP \(code): \(message)"
        }
    }
}
