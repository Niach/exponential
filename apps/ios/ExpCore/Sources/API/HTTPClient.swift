import Foundation

public final class HTTPClient: Sendable {
    private let auth: AuthRepository
    public let session: URLSession

    public init(auth: AuthRepository) {
        self.auth = auth
        let config = URLSessionConfiguration.default
        config.httpAdditionalHeaders = [
            "Accept": "application/json",
            // Client-version gate (EXP-104): every request through this client
            // advertises its build so the server can 426 out-of-date clients.
            "x-client-version": AppConstants.clientVersionHeaderValue,
        ]
        config.timeoutIntervalForRequest = 30
        // Bearer auth is used everywhere, so no cookie jar or response cache is
        // needed — and both leak across accounts on the same host: a Better-Auth
        // Set-Cookie from one user's sign-in would ride along on the next user's
        // requests, and a cached response could be replayed cross-auth. Kill
        // both (mirrors ShapeClient's cache-off stance).
        config.httpShouldSetCookies = false
        config.httpCookieStorage = nil
        config.urlCache = nil
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
        // Client-version gate (EXP-104): a 426 means this build is below the
        // server minimum. Trip the update gate (defensive decode — min/latest
        // may be absent, and the decode must never throw) and fall through to
        // the caller's existing error handling unchanged.
        if httpResponse.statusCode == 426 {
            let info = try? JSONDecoder().decode(ClientUpgradeResponse.self, from: data)
            UpdateGate.shared.trigger(min: info?.min, latest: info?.latest)
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
        // Better Auth's CSRF check 403s cross-origin POSTs that carry no
        // Origin header (MISSING_OR_NULL_ORIGIN); send the instance's own
        // origin like a same-origin browser request would.
        if let scheme = url.scheme, let host = url.host {
            let origin = url.port.map { "\(scheme)://\(host):\($0)" } ?? "\(scheme)://\(host)"
            req.setValue(origin, forHTTPHeaderField: "Origin")
        }
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

    // Rendered directly on the login screen (auth-config fetch failures), so
    // the description carries only the status — never the raw response body,
    // which stays in the associated value for debugging (EXP-219).
    public var errorDescription: String? {
        switch self {
        case .invalidResponse: "Invalid server response"
        case let .httpError(code, _): "Request failed (HTTP \(code))"
        }
    }
}
