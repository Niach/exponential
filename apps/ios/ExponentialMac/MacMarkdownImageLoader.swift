import AppKit
import ExpCore
import ExpUI
import Foundation

/// Resolves a stored (relative) attachment URL to absolute, once, at fetch time.
enum MacAttachmentURL {
    static func resolve(_ urlString: String, baseURL: URL?) -> URL? {
        if let url = URL(string: urlString), url.scheme != nil { return url }
        guard let baseURL else { return URL(string: urlString) }
        let base = baseURL.absoluteString.hasSuffix("/")
            ? String(baseURL.absoluteString.dropLast())
            : baseURL.absoluteString
        return URL(string: base + urlString)
    }
}

/// Process-wide decoded-image cache keyed by resolved absolute URL string.
final class MacImageCache: @unchecked Sendable {
    static let shared = MacImageCache()
    private let cache = NSCache<NSString, NSImage>()
    private init() { cache.totalCostLimit = 64 * 1024 * 1024 }

    func image(for key: String) -> NSImage? { cache.object(forKey: key as NSString) }
    func store(_ image: NSImage, for key: String) {
        let cost = Int(image.size.width * image.size.height * 4)
        cache.setObject(image, forKey: key as NSString, cost: max(cost, 1))
    }
}

/// Loads editor images: `draft://` placeholders resolve to in-memory pending
/// bytes; `/api/` URLs fetch with the account's auth header; everything else is
/// a plain URLSession fetch. Mirrors the iOS `AttachmentImageLoader`.
struct MacAttachmentImageLoader {
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
    let pendingImages: [String: PendingImage]

    enum LoadError: Error { case noData, decodeFailed }

    func load(_ urlString: String) async throws -> NSImage {
        if MarkdownImageUtils.isDraft(urlString) {
            guard let data = pendingImages[urlString]?.data, let image = NSImage(data: data) else {
                throw LoadError.noData
            }
            return image
        }
        guard let resolved = MacAttachmentURL.resolve(urlString, baseURL: baseURL) else {
            throw LoadError.noData
        }
        let key = resolved.absoluteString
        if let cached = MacImageCache.shared.image(for: key) { return cached }

        let data: Data
        if urlString.contains("/api/"), let httpClient, !accountId.isEmpty {
            (data, _) = try await httpClient.get(resolved, accountId: accountId)
        } else {
            (data, _) = try await URLSession.shared.data(from: resolved)
        }
        guard let image = NSImage(data: data) else { throw LoadError.decodeFailed }
        MacImageCache.shared.store(image, for: key)
        return image
    }
}
