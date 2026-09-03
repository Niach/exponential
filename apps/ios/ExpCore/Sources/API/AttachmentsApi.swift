import Foundation

// EXP-297 — issue attachments of every content type, inline images included
// (EXP-613: the legacy /images route's client is gone, this is the one path).
//
// Upload is a plain REST multipart route (tRPC's transport doesn't carry binary
// bodies well): POST /api/issues/{issueId}/files, ONE part named "file". The
// response shape is dictated by apps/web/src/lib/storage/issue-attachment-upload.ts
// (`width`/`height` ride along but are null for non-images, so they're simply
// not decoded here).
//
// Delete and the byte read go through the normal authed paths: the
// `attachments.delete` tRPC mutation and a bearer GET on /api/attachments/{id}.
public struct UploadedAttachment: Decodable, Sendable {
    public let id: String
    public let url: String
    public let filename: String
    public let contentType: String
    public let sizeBytes: Int

    public init(id: String, url: String, filename: String, contentType: String, sizeBytes: Int) {
        self.id = id
        self.url = url
        self.filename = filename
        self.contentType = contentType
        self.sizeBytes = sizeBytes
    }
}

private struct AttachmentIdInput: Encodable {
    let id: String
}

private struct EmptyResult: Decodable {}

public final class AttachmentsApi: Sendable {
    /// A 50 MB body over a phone uplink outlives the shared 30 s request budget
    /// by a wide margin — give upload and download their own five-minute window.
    private static let transferTimeout: TimeInterval = 300

    private let httpClient: HTTPClient
    private let trpc: TrpcClient
    private let auth: AuthRepository

    public init(httpClient: HTTPClient, trpc: TrpcClient, auth: AuthRepository) {
        self.httpClient = httpClient
        self.trpc = trpc
        self.auth = auth
    }

    // MARK: - Upload

    public func upload(
        accountId: String,
        issueId: String,
        data: Data,
        filename: String,
        contentType: String
    ) async throws -> UploadedAttachment {
        try await upload(
            accountId: accountId,
            path: "/api/issues/\(issueId)/files",
            data: data,
            filename: filename,
            contentType: contentType
        )
    }

    /// EXP-702: a steered image belongs to the SESSION, not to the issue the
    /// run happens to be about — it never shows up in the issue's Files
    /// section, and a batch/action run (which has no issue at all) can carry
    /// images too. Same multipart part name and response contract as the issue
    /// route; the server gates on session ownership.
    public func uploadSessionImage(
        accountId: String,
        sessionId: String,
        data: Data,
        filename: String,
        contentType: String
    ) async throws -> UploadedAttachment {
        try await upload(
            accountId: accountId,
            path: "/api/sessions/\(sessionId)/files",
            data: data,
            filename: filename,
            contentType: contentType
        )
    }

    private func upload(
        accountId: String,
        path: String,
        data: Data,
        filename: String,
        contentType: String
    ) async throws -> UploadedAttachment {
        guard let baseUrl = instanceUrl(for: accountId) else {
            throw AttachmentsError.noInstanceUrl
        }
        guard let url = URL(string: "\(baseUrl)\(path)") else {
            throw AttachmentsError.invalidUrl
        }

        // Hand-rolled body: the quoted `name="file"` form is the contract the
        // route parses (EXP-61).
        // Quotes/backslashes/CRLF in the user's filename would break the
        // quoted-string disposition — replaced like Android's
        // buildImageUploadBody does, regardless of caller sanitization.
        let safeFilename = filename.replacingOccurrences(
            of: "[\"\\\\\r\n]",
            with: "_",
            options: .regularExpression
        )
        let boundary = "Boundary-\(UUID().uuidString)"
        var body = Data()
        body.append(Data("--\(boundary)\r\n".utf8))
        body.append(Data(
            "Content-Disposition: form-data; name=\"file\"; filename=\"\(safeFilename)\"\r\n".utf8
        ))
        body.append(Data("Content-Type: \(contentType)\r\n\r\n".utf8))
        body.append(data)
        body.append(Data("\r\n--\(boundary)--\r\n".utf8))

        var request = httpClient.request(
            url,
            accountId: accountId,
            method: "POST",
            body: body,
            contentType: "multipart/form-data; boundary=\(boundary)"
        )
        request.setValue("\(body.count)", forHTTPHeaderField: "Content-Length")
        request.timeoutInterval = Self.transferTimeout

        let (responseData, response) = try await httpClient.perform(request, accountId: accountId)
        guard (200...299).contains(response.statusCode) else {
            let text = String(data: responseData, encoding: .utf8) ?? ""
            throw AttachmentsError.httpError(response.statusCode, text)
        }
        return try JSONDecoder().decode(UploadedAttachment.self, from: responseData)
    }

    // MARK: - Delete

    /// Member-level delete (apps/web/src/lib/trpc/attachments.ts). The server
    /// also rewrites every markdown reference to the row into a plain-text
    /// placeholder, so nothing is left pointing at a dead URL; the row's removal
    /// arrives here through Electric sync like any other write.
    public func delete(accountId: String, attachmentId: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "attachments.delete",
            input: AttachmentIdInput(id: attachmentId)
        )
    }

    // MARK: - Download

    /// Fetch an attachment's bytes. `relativeUrl` is the stored
    /// `/api/attachments/{id}` form; it is resolved against the account's
    /// instance exactly like the markdown image loader does, and the account's
    /// bearer only rides the request when the resolved URL really is on this
    /// instance's origin (a foreign host must never see the session token).
    public func download(accountId: String, relativeUrl: String) async throws -> Data {
        let base = instanceUrl(for: accountId).flatMap { URL(string: $0) }
        guard let resolved = Self.resolve(relativeUrl, baseURL: base) else {
            throw AttachmentsError.invalidUrl
        }

        let data: Data
        let statusCode: Int
        if Self.isSameOrigin(resolved, as: base) {
            var request = httpClient.request(resolved, accountId: accountId, contentType: nil)
            request.timeoutInterval = Self.transferTimeout
            let (body, response) = try await httpClient.perform(request, accountId: accountId)
            data = body
            statusCode = response.statusCode
        } else {
            var request = URLRequest(url: resolved)
            request.timeoutInterval = Self.transferTimeout
            let (body, response) = try await httpClient.session.data(for: request)
            data = body
            statusCode = (response as? HTTPURLResponse)?.statusCode ?? 0
        }

        guard (200...299).contains(statusCode) else {
            throw AttachmentsError.httpError(statusCode, String(data: data, encoding: .utf8) ?? "")
        }
        return data
    }

    // MARK: - URL helpers (mirrors the app's AttachmentURL)

    static func resolve(_ urlString: String, baseURL: URL?) -> URL? {
        if let url = URL(string: urlString), url.scheme != nil { return url }
        guard let baseURL else { return URL(string: urlString) }
        let base = baseURL.absoluteString.hasSuffix("/")
            ? String(baseURL.absoluteString.dropLast())
            : baseURL.absoluteString
        return URL(string: base + urlString)
    }

    static func isSameOrigin(_ url: URL, as base: URL?) -> Bool {
        guard let base,
              let scheme = url.scheme?.lowercased(),
              scheme == base.scheme?.lowercased(),
              let host = url.host?.lowercased(),
              host == base.host?.lowercased()
        else { return false }
        return effectivePort(of: url) == effectivePort(of: base)
    }

    private static func effectivePort(of url: URL) -> Int? {
        if let port = url.port { return port }
        switch url.scheme?.lowercased() {
        case "https": return 443
        case "http": return 80
        default: return nil
        }
    }

    private func instanceUrl(for accountId: String) -> String? {
        auth.accounts.first(where: { $0.id == accountId })?.instanceUrl
    }
}

/// Neutral storage-cap copy shown instead of the upload route's HTTP 412 body,
/// which carries purchase language ("Upgrade to upload more.") that must never
/// render in the iOS app (App Store 3.1.1 — EXP-216). Mirrors the failed-tile
/// wording in the main editor.
public let storageFullNeutralMessage = "Team storage is full."

public enum AttachmentsError: Error, LocalizedError, Sendable {
    case noInstanceUrl
    case invalidUrl
    case httpError(Int, String)

    public var errorDescription: String? {
        switch self {
        case .noInstanceUrl: "No instance URL configured"
        case .invalidUrl: "Invalid attachment URL"
        // The 412 body carries the team storage cap's purchase language
        // ("Upgrade to upload more."), which must never render on iOS
        // (App Store 3.1.1 — EXP-216). Keep the raw body on the case for
        // debugging and show the neutral copy instead.
        case let .httpError(code, message):
            code == 412 ? storageFullNeutralMessage : "File transfer failed: HTTP \(code) \(message)"
        }
    }
}
