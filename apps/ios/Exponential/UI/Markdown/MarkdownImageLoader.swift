import ExpUI
import ExpCore
import Foundation
import UIKit

/// Resolves a stored (relative) attachment URL to an absolute URL exactly once,
/// at fetch time. Blocks store the raw relative `/api/attachments/{id}` form so
/// `blocksToMarkdown` re-emits exactly what the backend expects.
enum AttachmentURL {
    static func resolve(_ urlString: String, baseURL: URL?) -> URL? {
        if let url = URL(string: urlString), url.scheme != nil { return url }
        guard let baseURL else { return URL(string: urlString) }
        let base = baseURL.absoluteString.hasSuffix("/")
            ? String(baseURL.absoluteString.dropLast())
            : baseURL.absoluteString
        return URL(string: base + urlString)
    }

    /// Whether `url` shares the signed-in instance's origin (scheme + host +
    /// port), i.e. whether the account's Better-Auth bearer may ride the
    /// request. Mirrors Android's `InstanceUrlInterceptor.resolveToken`
    /// instance-URL match: a markdown image pointing at any other host — even
    /// one whose path contains "/api/" — must be fetched anonymously, or the
    /// session token leaks to that host.
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
}

/// Process-wide decoded-image cache, keyed by resolved absolute URL string.
final class MarkdownImageCache: @unchecked Sendable {
    static let shared = MarkdownImageCache()

    private let cache = NSCache<NSString, UIImage>()

    private init() {
        cache.totalCostLimit = 64 * 1024 * 1024 // ~64 MB of decoded images
    }

    func image(for key: String) -> UIImage? {
        cache.object(forKey: key as NSString)
    }

    func store(_ image: UIImage, for key: String) {
        let cost = Int(image.size.width * image.size.height * image.scale * image.scale * 4)
        cache.setObject(image, forKey: key as NSString, cost: max(cost, 1))
    }
}

/// Loads attachment images for the editor: `draft://` placeholders resolve to
/// in-memory pending bytes; `/api/` URLs on the account's own instance fetch
/// with the account's auth header; everything else — including "/api/" paths on
/// foreign hosts — falls back to a plain `URLSession` fetch. Decoded results are
/// cached by resolved URL so re-appearances and the draft→real URL swap don't
/// trigger redundant downloads.
struct AttachmentImageLoader {
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
    let pendingImages: [String: PendingImage]

    enum LoadError: Error { case noData, decodeFailed }

    func load(_ urlString: String) async throws -> UIImage {
        if MarkdownImageUtils.isDraft(urlString) {
            guard let data = pendingImages[urlString]?.data,
                  let image = UIImage(data: data) else { throw LoadError.noData }
            return image
        }

        guard let resolved = AttachmentURL.resolve(urlString, baseURL: baseURL) else {
            throw LoadError.noData
        }
        let cacheKey = resolved.absoluteString
        if let cached = MarkdownImageCache.shared.image(for: cacheKey) { return cached }

        let data: Data
        if urlString.contains("/api/"),
           AttachmentURL.isSameOrigin(resolved, as: baseURL),
           let httpClient, !accountId.isEmpty {
            (data, _) = try await httpClient.get(resolved, accountId: accountId)
        } else {
            (data, _) = try await URLSession.shared.data(from: resolved)
        }
        guard let image = UIImage(data: data) else { throw LoadError.decodeFailed }
        MarkdownImageCache.shared.store(image, for: cacheKey)
        return image
    }
}
