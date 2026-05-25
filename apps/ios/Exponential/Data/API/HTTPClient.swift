import Foundation

final class HTTPClient: Sendable {
    private let auth: AuthRepository
    let session: URLSession

    init(auth: AuthRepository) {
        self.auth = auth
        let config = URLSessionConfiguration.default
        config.httpAdditionalHeaders = [
            "Accept": "application/json",
        ]
        config.timeoutIntervalForRequest = 30
        self.session = URLSession(configuration: config)
    }

    func request(_ url: URL, accountId: String, method: String = "GET", body: Data? = nil, contentType: String? = "application/json") -> URLRequest {
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

    func perform(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let (data, response) = try await session.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw HTTPError.invalidResponse
        }
        return (data, httpResponse)
    }

    func get(_ url: URL, accountId: String) async throws -> (Data, HTTPURLResponse) {
        try await perform(request(url, accountId: accountId))
    }

    func post(_ url: URL, accountId: String, body: Data) async throws -> (Data, HTTPURLResponse) {
        try await perform(request(url, accountId: accountId, method: "POST", body: body))
    }

    // Unauthenticated GET — used by AuthApi.fetchAuthConfig before any
    // account exists.
    func getUnauthenticated(_ url: URL) async throws -> (Data, HTTPURLResponse) {
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
    func postUnauthenticated(_ url: URL, body: Data) async throws -> (Data, HTTPURLResponse) {
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

enum HTTPError: Error, LocalizedError {
    case invalidResponse
    case httpError(Int, String)

    var errorDescription: String? {
        switch self {
        case .invalidResponse: "Invalid server response"
        case let .httpError(code, message): "HTTP \(code): \(message)"
        }
    }
}
