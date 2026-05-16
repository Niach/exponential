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

    func request(_ url: URL, method: String = "GET", body: Data? = nil, contentType: String? = "application/json") -> URLRequest {
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.httpBody = body
        if let contentType {
            request.setValue(contentType, forHTTPHeaderField: "Content-Type")
        }
        if let token = auth.token {
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

    func get(_ url: URL) async throws -> (Data, HTTPURLResponse) {
        try await perform(request(url))
    }

    func post(_ url: URL, body: Data) async throws -> (Data, HTTPURLResponse) {
        try await perform(request(url, method: "POST", body: body))
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
