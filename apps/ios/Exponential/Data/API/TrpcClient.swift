import Foundation

final class TrpcClient: Sendable {
    private let httpClient: HTTPClient
    private let auth: AuthRepository
    private let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }()

    init(httpClient: HTTPClient, auth: AuthRepository) {
        self.httpClient = httpClient
        self.auth = auth
    }

    func mutation<I: Encodable, O: Decodable>(path: String, input: I) async throws -> O {
        guard let baseUrl = auth.instanceUrl else {
            throw TrpcError.noInstanceUrl
        }
        guard let url = URL(string: "\(baseUrl)/api/trpc/\(path)") else {
            throw TrpcError.invalidUrl
        }

        let envelope = TrpcRequestEnvelope(json: input)
        let body = try JSONEncoder().encode(envelope)
        let (data, response) = try await httpClient.post(url, body: body)

        guard (200...299).contains(response.statusCode) else {
            let text = String(data: data, encoding: .utf8) ?? ""
            throw TrpcError.httpError(response.statusCode, text)
        }

        let wrapper = try JSONDecoder().decode(TrpcResponseEnvelope<O>.self, from: data)
        return wrapper.result.data.json
    }

    func mutationVoid<I: Encodable>(path: String, input: I) async throws {
        guard let baseUrl = auth.instanceUrl else {
            throw TrpcError.noInstanceUrl
        }
        guard let url = URL(string: "\(baseUrl)/api/trpc/\(path)") else {
            throw TrpcError.invalidUrl
        }

        let envelope = TrpcRequestEnvelope(json: input)
        let body = try JSONEncoder().encode(envelope)
        let (data, response) = try await httpClient.post(url, body: body)

        guard (200...299).contains(response.statusCode) else {
            let text = String(data: data, encoding: .utf8) ?? ""
            throw TrpcError.httpError(response.statusCode, text)
        }
        _ = data
    }
}

// MARK: - Envelope Types

private struct TrpcRequestEnvelope<T: Encodable>: Encodable {
    let json: T
}

private struct TrpcResponseEnvelope<T: Decodable>: Decodable {
    let result: TrpcResult<T>
}

private struct TrpcResult<T: Decodable>: Decodable {
    let data: TrpcData<T>
}

private struct TrpcData<T: Decodable>: Decodable {
    let json: T
}

enum TrpcError: Error, LocalizedError {
    case noInstanceUrl
    case invalidUrl
    case httpError(Int, String)

    var errorDescription: String? {
        switch self {
        case .noInstanceUrl: "No instance URL configured"
        case .invalidUrl: "Invalid URL"
        case let .httpError(code, message): "tRPC HTTP \(code): \(message)"
        }
    }
}
