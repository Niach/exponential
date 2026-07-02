import Foundation

public final class TrpcClient: Sendable {
    private let httpClient: HTTPClient
    private let auth: AuthRepository

    public init(httpClient: HTTPClient, auth: AuthRepository) {
        self.httpClient = httpClient
        self.auth = auth
    }

    /// Look up the instanceUrl for the given account. Every tRPC call needs
    /// this so the request routes to the right server.
    private func baseUrl(for accountId: String) throws -> String {
        guard let url = auth.accounts.first(where: { $0.id == accountId })?.instanceUrl else {
            throw TrpcError.noInstanceUrl
        }
        return url
    }

    public func mutation<I: Encodable, O: Decodable>(accountId: String, path: String, input: I) async throws -> O {
        let base = try baseUrl(for: accountId)
        guard let url = URL(string: "\(base)/api/trpc/\(path)") else {
            throw TrpcError.invalidUrl
        }

        let body = try JSONEncoder().encode(input)
        let (data, response) = try await httpClient.post(url, accountId: accountId, body: body)

        guard (200...299).contains(response.statusCode) else {
            let text = String(data: data, encoding: .utf8) ?? ""
            throw TrpcError.httpError(response.statusCode, text)
        }

        let wrapper = try JSONDecoder().decode(TrpcResponseEnvelope<O>.self, from: data)
        return wrapper.result.data
    }

    public func mutationVoid<I: Encodable>(accountId: String, path: String, input: I) async throws {
        let base = try baseUrl(for: accountId)
        guard let url = URL(string: "\(base)/api/trpc/\(path)") else {
            throw TrpcError.invalidUrl
        }

        let body = try JSONEncoder().encode(input)
        let (data, response) = try await httpClient.post(url, accountId: accountId, body: body)

        guard (200...299).contains(response.statusCode) else {
            let text = String(data: data, encoding: .utf8) ?? ""
            throw TrpcError.httpError(response.statusCode, text)
        }
        _ = data
    }

    /// GET an input-less tRPC `query` procedure and decode the same
    /// `{result:{data}}` envelope `mutation` uses. tRPC routes reads as GET, so
    /// POSTing to a `.query` returns 405 — use this for those.
    public func query<O: Decodable>(accountId: String, path: String) async throws -> O {
        let base = try baseUrl(for: accountId)
        guard let url = URL(string: "\(base)/api/trpc/\(path)") else {
            throw TrpcError.invalidUrl
        }

        let (data, response) = try await httpClient.get(url, accountId: accountId)

        guard (200...299).contains(response.statusCode) else {
            let text = String(data: data, encoding: .utf8) ?? ""
            throw TrpcError.httpError(response.statusCode, text)
        }

        let wrapper = try JSONDecoder().decode(TrpcResponseEnvelope<O>.self, from: data)
        return wrapper.result.data
    }

    /// GET a tRPC `query` procedure that takes an input. The server uses NO
    /// transformer (`initTRPC.context().create()`), so the input is the raw JSON
    /// value percent-encoded into `?input=…` (NOT the batched `{"0":{json}}`
    /// form).
    public func query<I: Encodable, O: Decodable>(accountId: String, path: String, input: I) async throws -> O {
        let base = try baseUrl(for: accountId)
        let json = try JSONEncoder().encode(input)
        guard let jsonString = String(data: json, encoding: .utf8) else {
            throw TrpcError.invalidUrl
        }
        // Percent-encode everything except RFC-3986 unreserved chars so the JSON
        // delimiters (`{ } " : , + & = ?`) survive intact (URLComponents would
        // leave `+` literal, which servers decode as a space).
        var allowed = CharacterSet.alphanumerics
        allowed.insert(charactersIn: "-._~")
        guard let encoded = jsonString.addingPercentEncoding(withAllowedCharacters: allowed),
              let url = URL(string: "\(base)/api/trpc/\(path)?input=\(encoded)") else {
            throw TrpcError.invalidUrl
        }

        let (data, response) = try await httpClient.get(url, accountId: accountId)

        guard (200...299).contains(response.statusCode) else {
            let text = String(data: data, encoding: .utf8) ?? ""
            throw TrpcError.httpError(response.statusCode, text)
        }

        let wrapper = try JSONDecoder().decode(TrpcResponseEnvelope<O>.self, from: data)
        return wrapper.result.data
    }
}

// MARK: - Response Envelope

private struct TrpcResponseEnvelope<T: Decodable>: Decodable {
    let result: TrpcResult<T>
}

private struct TrpcResult<T: Decodable>: Decodable {
    let data: T
}

public enum TrpcError: Error, LocalizedError, Sendable {
    case noInstanceUrl
    case invalidUrl
    case httpError(Int, String)

    public var errorDescription: String? {
        switch self {
        case .noInstanceUrl: "No instance URL configured"
        case .invalidUrl: "Invalid URL"
        case let .httpError(code, message): "tRPC HTTP \(code): \(message)"
        }
    }
}
