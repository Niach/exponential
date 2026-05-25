import Foundation

// Multipart-form POST to /api/issues/{issueId}/images.
//
// tRPC's transport doesn't carry binary attachments well, so the server
// exposes this as a plain REST route (apps/web/src/routes/api/issues/
// $issueId/images.ts). The shape of the response is dictated by that
// handler — see `UploadedImage` below.
struct UploadedImage: Decodable, Sendable {
    let id: String
    let url: String
    let filename: String
    let contentType: String
    let sizeBytes: Int
}

final class IssueImagesApi: Sendable {
    private let httpClient: HTTPClient
    private let auth: AuthRepository

    init(httpClient: HTTPClient, auth: AuthRepository) {
        self.httpClient = httpClient
        self.auth = auth
    }

    func upload(
        accountId: String,
        issueId: String,
        data: Data,
        filename: String,
        contentType: String
    ) async throws -> UploadedImage {
        guard let baseUrl = auth.accounts.first(where: { $0.id == accountId })?.instanceUrl else {
            throw IssueImagesError.noInstanceUrl
        }
        guard let url = URL(string: "\(baseUrl)/api/issues/\(issueId)/images") else {
            throw IssueImagesError.invalidUrl
        }

        let boundary = "Boundary-\(UUID().uuidString)"
        var body = Data()
        body.append("--\(boundary)\r\n")
        body.append(
            "Content-Disposition: form-data; name=\"file\"; filename=\"\(filename)\"\r\n"
        )
        body.append("Content-Type: \(contentType)\r\n\r\n")
        body.append(data)
        body.append("\r\n--\(boundary)--\r\n")

        var request = httpClient.request(
            url,
            accountId: accountId,
            method: "POST",
            body: body,
            contentType: "multipart/form-data; boundary=\(boundary)"
        )
        request.setValue("\(body.count)", forHTTPHeaderField: "Content-Length")

        let (responseData, response) = try await httpClient.perform(request)
        guard (200...299).contains(response.statusCode) else {
            let text = String(data: responseData, encoding: .utf8) ?? ""
            throw IssueImagesError.httpError(response.statusCode, text)
        }
        return try JSONDecoder().decode(UploadedImage.self, from: responseData)
    }
}

enum IssueImagesError: Error, LocalizedError {
    case noInstanceUrl
    case invalidUrl
    case httpError(Int, String)

    var errorDescription: String? {
        switch self {
        case .noInstanceUrl: "No instance URL configured"
        case .invalidUrl: "Invalid image upload URL"
        case let .httpError(code, message): "Image upload failed: HTTP \(code) \(message)"
        }
    }
}

private extension Data {
    mutating func append(_ string: String) {
        if let data = string.data(using: .utf8) { append(data) }
    }
}
