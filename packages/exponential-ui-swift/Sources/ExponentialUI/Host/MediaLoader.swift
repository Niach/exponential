import SwiftUI
import ImageIO
import UniformTypeIdentifiers
#if canImport(UIKit)
import UIKit
typealias PlatformImage = UIImage
#elseif canImport(AppKit)
import AppKit
typealias PlatformImage = NSImage
#endif

/// The painter's media loader (VAPP-91): every Image / Avatar / Video poster
/// (and a Video / AudioPlayer `src` that needs a fetch, `playableURL`) loads through the host's `mediaRequest` (a `URLRequest` that may carry
/// auth headers, e.g. `/api/attachments`), never `AsyncImage`. Decoded
/// images are cached per url + headers. VAPP-103: every load fits the
/// contract's `media.limits` (read once from the core): a Content-Length
/// over `maxBytes` is refused up front and the body is capped as it
/// streams, `timeoutMs` bounds the whole request, and width × height come
/// from the image header BEFORE anything decodes; a load over a limit fails
/// like a 404 (the fallback paints).
@MainActor
final class MediaLoader {
    static let shared = MediaLoader()
    private let cache = NSCache<NSString, PlatformImage>()
    private var inflight: [String: Task<PlatformImage?, Never>] = [:]
    private var files: [String: URL] = [:]
    private var fileLoads: [String: Task<URL?, Never>] = [:]
    var session: URLSession = .shared
    var limits: MediaLimits = .contract

    /// Why a load failed (tests; the painter only sees nil).
    enum Failure: Error, Equatable {
        case http(Int)
        case tooLarge(Int64)
        case tooManyPixels(Int, Int)
        case undecodable
        case timedOut
    }

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
        let limits = self.limits
        let task = Task<PlatformImage?, Never> {
            try? await Self.fetch(request, session: session, limits: limits).get()
        }
        inflight[key] = task
        let image = await task.value
        inflight[key] = nil
        if let image { cache.setObject(image, forKey: key as NSString) }
        return image
    }

    /// One picture load under `limits`: the bytes (`fetchData`), then the
    /// header-checked decode.
    nonisolated static func fetch(_ request: URLRequest, session: URLSession, limits: MediaLimits) async -> Result<PlatformImage, Failure> {
        switch await fetchData(request, session: session, limits: limits) {
        case .success(let (data, _)): return decode(data, limits: limits)
        case .failure(let f): return .failure(f)
        }
    }

    /// The bytes (and MIME type) of one request under `limits`: the whole
    /// request races `timeoutMs`, the body is capped at `maxBytes`.
    nonisolated static func fetchData(_ request: URLRequest, session: URLSession, limits: MediaLimits) async -> Result<(Data, String?), Failure> {
        var request = request
        let timeout = max(limits.timeoutMs, 1) / 1000
        request.timeoutInterval = timeout
        do {
            let got = try await withThrowingTaskGroup(of: (Data, String?).self) { group in
                group.addTask { try await body(request, session: session, maxBytes: limits.maxBytes) }
                group.addTask {
                    try await Task.sleep(nanoseconds: UInt64(timeout * 1_000_000_000))
                    throw Failure.timedOut
                }
                defer { group.cancelAll() }
                guard let first = try await group.next() else { throw Failure.timedOut }
                return first
            }
            return .success(got)
        } catch let f as Failure {
            return .failure(f)
        } catch {
            return .failure(.timedOut)
        }
    }

    /// The body and its MIME type, refused up front when its Content-Length
    /// is over `maxBytes` and cut off as soon as it streams past it.
    nonisolated static func body(_ request: URLRequest, session: URLSession, maxBytes: Int64) async throws -> (Data, String?) {
        let (bytes, response) = try await session.bytes(for: request)
        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) { throw Failure.http(http.statusCode) }
        if response.expectedContentLength > maxBytes { throw Failure.tooLarge(response.expectedContentLength) }
        var data = Data()
        if response.expectedContentLength > 0 { data.reserveCapacity(Int(response.expectedContentLength)) }
        var chunk = [UInt8]()
        chunk.reserveCapacity(64 * 1024)
        for try await byte in bytes {
            chunk.append(byte)
            if chunk.count == 64 * 1024 {
                data.append(contentsOf: chunk)
                chunk.removeAll(keepingCapacity: true)
                if Int64(data.count) > maxBytes { throw Failure.tooLarge(Int64(data.count)) }
            }
        }
        data.append(contentsOf: chunk)
        if Int64(data.count) > maxBytes { throw Failure.tooLarge(Int64(data.count)) }
        return (data, response.mimeType)
    }

    // MARK: - Video / AudioPlayer sources

    /// How a policed Video / AudioPlayer request plays (React's `<video
    /// src>`): a request without headers streams straight into the player;
    /// one that carries headers (`/api/attachments` auth) or a `data:` url
    /// is fetched under `media.limits` into a temporary file first (React's
    /// blob url).
    enum Playback: Equatable {
        case stream(URL)
        case fetch(URLRequest)
    }

    nonisolated static func playback(_ request: URLRequest) -> Playback? {
        guard let url = request.url, let scheme = url.scheme?.lowercased() else { return nil }
        if scheme != "data", (request.allHTTPHeaderFields ?? [:]).isEmpty { return .stream(url) }
        return .fetch(request)
    }

    /// The url a player opens for a policed request: the stream itself, or
    /// the fetched file (cached per url + headers); nil when the fetch failed.
    func playableURL(_ request: URLRequest) async -> URL? {
        switch Self.playback(request) {
        case .stream(let url): return url
        case .fetch(let request): return await file(request)
        case nil: return nil
        }
    }

    private func file(_ request: URLRequest) async -> URL? {
        let key = Self.key(request)
        if let hit = files[key] { return hit }
        if let running = fileLoads[key] { return await running.value }
        let session = self.session
        let limits = self.limits
        let task = Task<URL?, Never> {
            guard case .success(let (data, mime)) = await Self.fetchData(request, session: session, limits: limits) else { return nil }
            let ext = mime.flatMap { UTType(mimeType: $0)?.preferredFilenameExtension } ?? request.url?.pathExtension.nilIfEmpty ?? "mp4"
            let dir = FileManager.default.temporaryDirectory.appendingPathComponent("exponential-ui-media", isDirectory: true)
            let url = dir.appendingPathComponent("\(UUID().uuidString).\(ext)")
            do {
                try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
                try data.write(to: url)
                return url
            } catch {
                return nil
            }
        }
        fileLoads[key] = task
        let url = await task.value
        fileLoads[key] = nil
        if let url { files[key] = url }
        return url
    }

    /// Width × height from the image header, nothing decoded: PNG, GIF,
    /// BMP, WebP and JPEG read directly (the core's `image_dimensions`
    /// rule, so a truncated header still answers), anything else from the
    /// ImageIO properties; nil when neither reads it.
    nonisolated static func pixelSize(_ data: Data) -> (Int, Int)? {
        if let d = headerSize([UInt8](data.prefix(64 * 1024))) { return d }
        guard let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              CGImageSourceGetCount(source) > 0,
              let props = CGImageSourceCopyPropertiesAtIndex(source, 0, [kCGImageSourceShouldCache: false] as CFDictionary) as? [CFString: Any],
              let w = (props[kCGImagePropertyPixelWidth] as? NSNumber)?.intValue,
              let h = (props[kCGImagePropertyPixelHeight] as? NSNumber)?.intValue else { return nil }
        return (w, h)
    }

    nonisolated static func headerSize(_ b: [UInt8]) -> (Int, Int)? {
        func be16(_ i: Int) -> Int? { i + 2 <= b.count ? Int(b[i]) << 8 | Int(b[i + 1]) : nil }
        func le16(_ i: Int) -> Int? { i + 2 <= b.count ? Int(b[i + 1]) << 8 | Int(b[i]) : nil }
        func be32(_ i: Int) -> Int? { i + 4 <= b.count ? (0..<4).reduce(0) { $0 << 8 | Int(b[i + $1]) } : nil }
        func le32(_ i: Int) -> Int? { i + 4 <= b.count ? (0..<4).reversed().reduce(0) { $0 << 8 | Int(b[i + $1]) } : nil }
        func le24(_ i: Int) -> Int? { i + 3 <= b.count ? Int(b[i]) | Int(b[i + 1]) << 8 | Int(b[i + 2]) << 16 : nil }
        func starts(_ p: [UInt8]) -> Bool { b.count >= p.count && Array(b[0..<p.count]) == p }
        if starts([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            guard let w = be32(16), let h = be32(20) else { return nil }
            return (w, h)
        }
        if starts(Array("GIF87a".utf8)) || starts(Array("GIF89a".utf8)) {
            guard let w = le16(6), let h = le16(8) else { return nil }
            return (w, h)
        }
        if starts(Array("BM".utf8)) {
            guard let w = le32(18), let h = le32(22) else { return nil }
            return (w, Int(abs(Int32(truncatingIfNeeded: h))))
        }
        if b.count >= 30, starts(Array("RIFF".utf8)), Array(b[8..<12]) == Array("WEBP".utf8) {
            switch String(decoding: b[12..<16], as: UTF8.self) {
            case "VP8 ":
                guard let w = le16(26), let h = le16(28) else { return nil }
                return (w & 0x3fff, h & 0x3fff)
            case "VP8L":
                guard let v = le32(21) else { return nil }
                return ((v & 0x3fff) + 1, ((v >> 14) & 0x3fff) + 1)
            case "VP8X":
                guard let w = le24(24), let h = le24(27) else { return nil }
                return (w + 1, h + 1)
            default:
                return nil
            }
        }
        if starts([0xFF, 0xD8]) {
            var i = 2
            while i + 4 <= b.count {
                if b[i] != 0xFF { i += 1; continue }
                let marker = b[i + 1]
                if marker == 0xFF { i += 1; continue }
                if marker == 0xD8 || marker == 0x01 || (0xD0...0xD7).contains(marker) { i += 2; continue }
                guard let len = be16(i + 2) else { return nil }
                // SOF0…SOF15 except DHT (c4), JPG (c8), DAC (cc).
                if (0xC0...0xCF).contains(marker), marker != 0xC4, marker != 0xC8, marker != 0xCC {
                    guard let h = be16(i + 5), let w = be16(i + 7) else { return nil }
                    return (w, h)
                }
                i += 2 + len
            }
            return nil
        }
        return nil
    }

    /// Decode `data` when it fits: the byte cap, then the header's pixel
    /// count against `maxPixels` BEFORE decoding (an unreadable header
    /// never decodes), then one decode at the full size.
    nonisolated static func decode(_ data: Data, limits: MediaLimits) -> Result<PlatformImage, Failure> {
        if Int64(data.count) > limits.maxBytes { return .failure(.tooLarge(Int64(data.count))) }
        guard let (w, h) = pixelSize(data), w > 0, h > 0 else { return .failure(.undecodable) }
        if Int64(w) * Int64(h) > limits.maxPixels { return .failure(.tooManyPixels(w, h)) }
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return .failure(.undecodable) }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: max(w, h),
        ]
        guard let cg = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return .failure(.undecodable) }
        #if canImport(UIKit)
        return .success(UIImage(cgImage: cg))
        #else
        return .success(NSImage(cgImage: cg, size: NSSize(width: cg.width, height: cg.height)))
        #endif
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

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
