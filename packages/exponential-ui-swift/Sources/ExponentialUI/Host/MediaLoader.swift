import SwiftUI
#if canImport(UIKit)
import UIKit
typealias PlatformImage = UIImage
#elseif canImport(AppKit)
import AppKit
typealias PlatformImage = NSImage
#endif

/// The painter's image loader (VAPP-91): every Image / Avatar / Video poster
/// loads through the host's `mediaRequest` (a `URLRequest` that may carry
/// auth headers, e.g. `/api/attachments`), never `AsyncImage`. Decoded
/// images are cached per url + headers.
@MainActor
final class MediaLoader {
    static let shared = MediaLoader()
    private let cache = NSCache<NSString, PlatformImage>()
    private var inflight: [String: Task<PlatformImage?, Never>] = [:]
    var session: URLSession = .shared

    static func key(_ request: URLRequest) -> String {
        let headers = (request.allHTTPHeaderFields ?? [:]).sorted { $0.key < $1.key }.map { "\($0.key)=\($0.value)" }.joined(separator: "&")
        return "\(request.url?.absoluteString ?? "")|\(headers)"
    }

    func cached(_ request: URLRequest) -> PlatformImage? {
        cache.object(forKey: Self.key(request) as NSString)
    }

    func load(_ request: URLRequest) async -> PlatformImage? {
        let key = Self.key(request)
        if let hit = cache.object(forKey: key as NSString) { return hit }
        if let running = inflight[key] { return await running.value }
        let session = self.session
        let task = Task<PlatformImage?, Never> {
            guard let (data, response) = try? await session.data(for: request) else { return nil }
            if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) { return nil }
            return PlatformImage(data: data)
        }
        inflight[key] = task
        let image = await task.value
        inflight[key] = nil
        if let image { cache.setObject(image, forKey: key as NSString) }
        return image
    }
}

/// An image loaded through the host's media request; `placeholder` while
/// loading, failed, or without a request.
struct MediaImage<Content: View, Placeholder: View>: View {
    let request: URLRequest?
    @ViewBuilder let content: (Image) -> Content
    @ViewBuilder let placeholder: () -> Placeholder
    @State private var image: PlatformImage?
    @State private var loadedKey: String?

    var body: some View {
        let key = request.map(MediaLoader.key)
        Group {
            if let shown = (key != nil && loadedKey == key) ? image : request.flatMap({ MediaLoader.shared.cached($0) }) {
                content(Self.swiftUI(shown))
            } else {
                placeholder()
            }
        }
        .task(id: key) {
            guard let request else {
                image = nil
                loadedKey = nil
                return
            }
            let loaded = await MediaLoader.shared.load(request)
            image = loaded
            loadedKey = key
        }
    }

    static func swiftUI(_ image: PlatformImage) -> Image {
        #if canImport(UIKit)
        Image(uiImage: image)
        #else
        Image(nsImage: image)
        #endif
    }
}
