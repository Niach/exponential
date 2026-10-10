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
/// (and a Video / AudioPlayer `src`, `playable`) loads through the host's
/// `mediaRequest` (a `URLRequest` that may carry auth headers, e.g.
/// `/api/attachments`), never `AsyncImage`. Decoded images are cached per
/// url + headers. VAPP-103: every picture load fits the contract's
/// `media.limits` (read once from the core): a Content-Length over
/// `maxBytes` is refused up front and the body is capped as it streams,
/// `timeoutMs` bounds the whole request, and width × height come from the
/// image header BEFORE anything decodes; a load over a limit fails like a
/// 404 (the fallback paints). Every redirect hop passes the media policy
/// again (`policeRedirect`: schemes, hosts, no https→http downgrade, the
/// headers rebuilt from the rules the new url matches).
@MainActor
final class MediaLoader {
    static let shared = MediaLoader()
    private let cache = NSCache<NSString, PlatformImage>()
    private var inflight: [String: Task<PlatformImage?, Never>] = [:]
    /// The fetched `data:` media files, least recently used first.
    private var files: [(key: String, url: URL)] = []
    private var fileUsers: [String: Int] = [:]
    private var fileLoads: [String: Task<URL?, Never>] = [:]
    /// The media directory was emptied (once per process, on the first fetch).
    var clearedDirectory = false
    /// How many fetched media files stay on disk (the oldest unused one is
    /// deleted past it).
    var maxFiles = 8
    var session: URLSession = .shared
    var limits: MediaLimits = .contract

    /// Why a load failed (tests; the painter only sees nil).
    enum Failure: Error, Equatable {
        case http(Int)
        case tooLarge(Int64)
        case tooManyPixels(Int, Int)
        case undecodable
        case timedOut
        /// A redirect the media policy refused.
        case denied
    }

    static func key(_ request: URLRequest) -> String {
        let headers = (request.allHTTPHeaderFields ?? [:]).sorted { $0.key < $1.key }.map { "\($0.key)=\($0.value)" }.joined(separator: "&")
        return "\(request.url?.absoluteString ?? "")|\(headers)"
    }

    func cached(_ request: URLRequest) -> PlatformImage? {
        cache.object(forKey: Self.key(request) as NSString)
    }

    func load(_ request: URLRequest, options: MediaOptions?) async -> PlatformImage? {
        let key = Self.key(request)
        if let hit = cache.object(forKey: key as NSString) { return hit }
        if let running = inflight[key] { return await running.value }
        let session = self.session
        let limits = self.limits
        let task = Task<PlatformImage?, Never>.detached {
            try? await Self.fetch(request, session: session, limits: limits, options: options).get()
        }
        inflight[key] = task
        let image = await task.value
        inflight[key] = nil
        if let image { cache.setObject(image, forKey: key as NSString) }
        return image
    }

    /// One picture load under `limits`: the bytes (`fetchData`), then the
    /// header-checked decode.
    nonisolated static func fetch(_ request: URLRequest, session: URLSession, limits: MediaLimits, options: MediaOptions? = nil) async -> Result<PlatformImage, Failure> {
        switch await fetchData(request, session: session, limits: limits, options: options) {
        case .success(let (data, _)): return decode(data, limits: limits)
        case .failure(let f): return .failure(f)
        }
    }

    /// The bytes (and MIME type) of one request under `limits`: the whole
    /// request races `timeoutMs`, the body is capped at `maxBytes`, every
    /// redirect hop is policed under `options`.
    nonisolated static func fetchData(_ request: URLRequest, session: URLSession, limits: MediaLimits, options: MediaOptions? = nil) async -> Result<(Data, String?), Failure> {
        let timeout = max(limits.timeoutMs, 1) / 1000
        return await bounded(timeout) {
            var request = request
            request.timeoutInterval = timeout
            return try await body(request, session: session, maxBytes: limits.maxBytes, options: options)
        }
    }

    /// `work` raced against `timeout` seconds.
    nonisolated static func bounded<T: Sendable>(_ timeout: Double, _ work: @escaping @Sendable () async throws -> T) async -> Result<T, Failure> {
        do {
            let got = try await withThrowingTaskGroup(of: T.self) { group in
                group.addTask { try await work() }
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
    nonisolated static func body(_ request: URLRequest, session: URLSession, maxBytes: Int64, options: MediaOptions? = nil) async throws -> (Data, String?) {
        let police = RedirectPolice(request, options: options)
        let (bytes, response) = try await police.bytes(session, request)
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

    /// A redirect hop from `from` to `to` under the media policy: the new
    /// url must pass the media schemes / hosts (`policyMediaRequest` with
    /// the host's options), an https request never continues over http,
    /// and the hop's headers are REBUILT from the rules the new url matches
    /// (a header whose rule no longer matches is dropped). nil = refused.
    /// `keep` names request headers that are not policy headers (a probe's
    /// `Range`) and carry over.
    nonisolated static func policeRedirect(from: URLRequest, to: URLRequest, options: MediaOptions?, keep: [String] = []) -> URLRequest? {
        guard let url = to.url, let scheme = url.scheme?.lowercased() else { return nil }
        if from.url?.scheme?.lowercased() == "https", scheme != "https" { return nil }
        guard var next = policyMediaRequest(url.absoluteString, options: options), mediaAllowed(next, options: options) else { return nil }
        next.httpMethod = to.httpMethod
        next.timeoutInterval = to.timeoutInterval
        for name in keep {
            if let v = from.value(forHTTPHeaderField: name) { next.setValue(v, forHTTPHeaderField: name) }
        }
        return next
    }

    // MARK: - Video / AudioPlayer sources

    /// How a policed Video / AudioPlayer request plays (React's `<video
    /// src>`): an http(s) request STREAMS into the player with its headers
    /// (`catalog/host.json`: the byte limits do not apply to Video / Audio);
    /// a `data:` url is decoded into a temporary file first (bounded by the
    /// message size); any other scheme the host allowed opens as is.
    enum Playback: Equatable {
        case stream(URLRequest)
        case file(URLRequest)
    }

    nonisolated static func playback(_ request: URLRequest) -> Playback? {
        guard let scheme = request.url?.scheme?.lowercased() else { return nil }
        return scheme == "data" ? .file(request) : .stream(request)
    }

    /// What a player opens: the url, the headers its requests carry, and
    /// the fetched file's cache key (`release` it when the player goes).
    struct Playable: Equatable {
        let url: URL
        var headers: [String: String] = [:]
        var fileKey: String?
    }

    /// The player's source for a policed request, nil when it is refused
    /// or failed. An http(s) stream's redirect chain is resolved FIRST by a
    /// policed probe (`resolveStream`), so the player opens the final url
    /// with the final hop's headers; AVFoundation follows any LATER
    /// redirect of that url on its own (it has no redirect hook), so a
    /// server that redirects the probe and the player differently is not
    /// re-policed. A fetched file is retained until `release`.
    func playable(_ request: URLRequest, kind: MediaKind, options: MediaOptions?) async -> Playable? {
        switch Self.playback(request) {
        case .stream(let request):
            guard let scheme = request.url?.scheme?.lowercased(), scheme == "http" || scheme == "https" else {
                return request.url.map { Playable(url: $0) }
            }
            return await Self.resolveStream(request, session: session, limits: limits, options: options)
        case .file(let request):
            let key = Self.key(request)
            guard let url = await file(request, kind: kind) else { return nil }
            return Playable(url: url, fileKey: key)
        case nil:
            return nil
        }
    }

    /// The final url + headers of an http(s) stream: a `Range: bytes=0-0`
    /// GET under `timeoutMs` whose every redirect hop is policed; its body
    /// is never read. nil when a hop is refused or the server answers an
    /// error.
    nonisolated static func resolveStream(_ request: URLRequest, session: URLSession, limits: MediaLimits, options: MediaOptions?) async -> Playable? {
        let timeout = max(limits.timeoutMs, 1) / 1000
        let got = await bounded(timeout) { () -> Playable in
            var probe = request
            probe.timeoutInterval = timeout
            probe.setValue("bytes=0-0", forHTTPHeaderField: "Range")
            let police = RedirectPolice(probe, options: options, keep: ["Range"])
            let (bytes, response) = try await police.bytes(session, probe)
            bytes.task.cancel()
            if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode), http.statusCode != 416 { throw Failure.http(http.statusCode) }
            let last = police.current
            guard let url = response.url ?? last.url else { throw Failure.denied }
            var headers = last.allHTTPHeaderFields ?? [:]
            headers = headers.filter { $0.key.lowercased() != "range" }
            return Playable(url: url, headers: headers)
        }
        return try? got.get()
    }

    /// A player stopped using `playable`: its fetched file is deleted once
    /// no other player uses it.
    func release(_ playable: Playable?) {
        guard let key = playable?.fileKey else { return }
        let left = (fileUsers[key] ?? 1) - 1
        if left > 0 {
            fileUsers[key] = left
            return
        }
        fileUsers[key] = nil
        if let i = files.firstIndex(where: { $0.key == key }) {
            try? FileManager.default.removeItem(at: files[i].url)
            files.remove(at: i)
        }
    }

    /// The fetched files on disk (tests).
    var fileURLs: [URL] { files.map(\.url) }

    nonisolated static var mediaDirectory: URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("exponential-ui-media", isDirectory: true)
    }

    /// The fetched file of `request`, retained for the caller (`release`).
    private func file(_ request: URLRequest, kind: MediaKind) async -> URL? {
        let key = Self.key(request)
        if let i = files.firstIndex(where: { $0.key == key }) {
            let hit = files.remove(at: i)
            if FileManager.default.fileExists(atPath: hit.url.path) {
                files.append(hit)
                fileUsers[key, default: 0] += 1
                return hit.url
            }
        }
        if let running = fileLoads[key] {
            guard let url = await running.value else { return nil }
            fileUsers[key, default: 0] += 1
            return url
        }
        let session = self.session
        let clear = !clearedDirectory
        clearedDirectory = true
        // A data: url is bounded by the message size, not media.limits.
        var limits = self.limits
        limits.maxBytes = .max
        let task = Task<URL?, Never>.detached {
            let dir = Self.mediaDirectory
            if clear { try? FileManager.default.removeItem(at: dir) }
            guard case .success(let (data, mime)) = await Self.fetchData(request, session: session, limits: limits) else { return nil }
            let url = dir.appendingPathComponent("\(UUID().uuidString).\(Self.fileExtension(mime: mime, data: data, kind: kind))")
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
        if let url {
            files.append((key, url))
            fileUsers[key, default: 0] += 1
            evictFiles()
        }
        return url
    }

    /// Delete the least recently used files no player holds past `maxFiles`.
    private func evictFiles() {
        var excess = files.count - maxFiles
        var i = 0
        while excess > 0, i < files.count {
            if (fileUsers[files[i].key] ?? 0) == 0 {
                try? FileManager.default.removeItem(at: files[i].url)
                files.remove(at: i)
                excess -= 1
            } else {
                i += 1
            }
        }
    }

    /// The temporary file's extension AVFoundation reads the container
    /// from: the MIME type's when it names a known audio / video type, else
    /// sniffed from the leading bytes, else by the component (AudioPlayer
    /// `m4a`, Video `mp4`).
    nonisolated static func fileExtension(mime: String?, data: Data, kind: MediaKind) -> String {
        let known: [String: String] = [
            "audio/mpeg": "mp3", "audio/mp3": "mp3", "audio/mp4": "m4a", "audio/x-m4a": "m4a", "audio/m4a": "m4a", "audio/aac": "aac",
            "audio/wav": "wav", "audio/x-wav": "wav", "audio/wave": "wav", "audio/flac": "flac", "audio/x-flac": "flac",
            "audio/x-caf": "caf", "audio/aiff": "aiff", "audio/x-aiff": "aiff",
            "video/mp4": "mp4", "video/quicktime": "mov", "video/x-m4v": "m4v", "video/3gpp": "3gp",
        ]
        if let mime = mime?.lowercased().split(separator: ";").first.map({ $0.trimmingCharacters(in: .whitespaces) }), let ext = known[mime] { return ext }
        let b = [UInt8](data.prefix(16))
        func at(_ i: Int, _ s: String) -> Bool { b.count >= i + s.utf8.count && Array(b[i..<i + s.utf8.count]) == Array(s.utf8) }
        if at(0, "ID3") || (b.count >= 2 && b[0] == 0xFF && (b[1] & 0xE6) == 0xE2) { return "mp3" }
        if b.count >= 2, b[0] == 0xFF, (b[1] & 0xF6) == 0xF0 { return "aac" }
        if at(0, "RIFF"), at(8, "WAVE") { return "wav" }
        if at(0, "fLaC") { return "flac" }
        if at(0, "caff") { return "caf" }
        if at(0, "FORM"), at(8, "AIFF") { return "aiff" }
        if at(4, "ftyp") {
            if at(8, "M4A ") || at(8, "M4B ") { return "m4a" }
            if at(8, "qt  ") { return "mov" }
            if at(8, "M4V ") { return "m4v" }
            return kind == .audio ? "m4a" : "mp4"
        }
        return kind == .audio ? "m4a" : "mp4"
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
    /// The media policy its redirects pass (`SurfaceModel.mediaPolicy`).
    let options: MediaOptions?
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
            let loaded = await MediaLoader.shared.load(request, options: options)
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

/// Which player a source opens in (its temporary file's fallback type).
enum MediaKind: Sendable {
    case video, audio
}

/// The per-request redirect check (`MediaLoader.policeRedirect`): a refused
/// hop ends the load (`denied`); `current` is the last request sent.
final class RedirectPolice: NSObject, URLSessionTaskDelegate, @unchecked Sendable {
    private let lock = NSLock()
    private let options: MediaOptions?
    private let keep: [String]
    private var _current: URLRequest
    private var _denied = false

    init(_ request: URLRequest, options: MediaOptions?, keep: [String] = []) {
        self.options = options
        self.keep = keep
        _current = request
    }

    var current: URLRequest { lock.withLock { _current } }

    /// `session.bytes(for:)` with this check on every hop; a refused hop
    /// throws `denied` (a refused redirect would otherwise surface as its
    /// 3xx response).
    func bytes(_ session: URLSession, _ request: URLRequest) async throws -> (URLSession.AsyncBytes, URLResponse) {
        do {
            let got = try await session.bytes(for: request, delegate: self)
            if denied {
                got.0.task.cancel()
                throw MediaLoader.Failure.denied
            }
            return got
        } catch {
            if denied { throw MediaLoader.Failure.denied }
            throw error
        }
    }
    var denied: Bool { lock.withLock { _denied } }

    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        let next: URLRequest? = lock.withLock {
            let next = MediaLoader.policeRedirect(from: _current, to: request, options: options, keep: keep)
            if let next { _current = next } else { _denied = true }
            return next
        }
        if next == nil { task.cancel() }
        completionHandler(next)
    }
}
