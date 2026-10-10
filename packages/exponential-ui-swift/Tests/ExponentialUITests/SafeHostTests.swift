import XCTest
import SwiftUI
import ImageIO
import AVFoundation
import UniformTypeIdentifiers
import ExponentialUICore
import ExponentialUIPrimitives
@testable import ExponentialUI

/// VAPP-103 "safe hosts": every href through the URL policy, every src
/// through the media request (no raw-url fallback), the image loader's
/// `media.limits`, `onPaintError` → RENDER_FAILED, and the markdown painter
/// on the same rules.
@MainActor
final class SafeHostTests: XCTestCase {
    func msg(_ kind: String, _ body: JSONValue) -> String {
        JSONValue.object(["version": .string("v0.9"), kind: body]).json
    }

    func settle() async {
        for _ in 0..<5 { await Task.yield() }
        try? await Task.sleep(for: .milliseconds(20))
    }

    @discardableResult
    func paint(_ m: SurfaceModel, width: CGFloat = 390, height: CGFloat = 900) -> Set<Int> {
        m.setViewport(width: width, height: height)
        var painted = Set<Int>()
        m.paintProbe = { painted.insert($0) }
        defer { m.paintProbe = nil }
        let root = ExponentialSurface(model: m).frame(width: width)
        #if canImport(AppKit)
        let view = NSHostingView(rootView: root)
        view.frame = CGRect(x: 0, y: 0, width: width, height: max(m.surfaceSize.height, height))
        view.layoutSubtreeIfNeeded()
        _ = view.fittingSize
        #else
        // Outside a window a hosting view never evaluates its bodies.
        renderInWindow(root, size: CGSize(width: width, height: max(m.surfaceSize.height, height)))
        #endif
        return painted
    }

    // MARK: - hrefs

    func testDeniedHrefsNeverOpen() throws {
        // A bare plugin's default opener and a closure host both pass the policy.
        var opened: [String] = []
        let closure = ClosureHost(urls: { opened.append($0) })
        closure.openUrl("javascript:alert(1)")
        closure.openUrl("/relative/without/base")
        closure.openUrl("file:///etc/passwd")
        closure.openUrl("https://exponential.at/x")
        XCTAssertEqual(opened, ["https://exponential.at/x"])

        // The model's openUrl events go through the policy before the host.
        let m = try SurfaceModel(id: "l", options: SurfaceOptions(), host: closure)
        XCTAssertFalse(m.openLink("javascript:alert(2)"))
        XCTAssertNil(m.linkHref("vbscript:x"))
        XCTAssertTrue(m.openLink("mailto:a@b.c"))
        XCTAssertEqual(opened.last, "mailto:a@b.c")

        // Hosts allow-lists hold; relative hrefs resolve against the media base.
        closure.urlPolicy = UrlPolicy(hosts: ["*.exponential.at"])
        closure.mediaOptions = MediaOptions(baseUrl: "https://app.exponential.at")
        XCTAssertNil(m.linkHref("https://evil.example/"))
        XCTAssertEqual(m.linkHref("/t/acme"), "https://app.exponential.at/t/acme")

        // ExponentialHost: a denied href opens nothing.
        var hostOpened: [URL] = []
        let host = ExponentialHost(HostOptions(policy: HostPolicy(openUrl: { hostOpened.append($0) })))
        XCTAssertFalse(host.openURL("javascript:alert(1)"))
        XCTAssertFalse(host.openURL("x/y"), "relative without a base is denied")
        XCTAssertNil(host.linkHref("data:text/html,<script>"))
        XCTAssertTrue(host.openURL("https://exponential.at"))
        XCTAssertEqual(hostOpened.count, 1)
    }

    // MARK: - srcs

    final class RawPlugin: HostPlugin {
        var raw: String?
        var rewrite: [String: String] = [:]
        var mediaOptions: MediaOptions?
        func mediaRequest(_ src: String) -> URLRequest? {
            if let raw { return URLRequest(url: URL(string: raw)!) }
            return policyMediaRequest(resolveUrl(src), options: mediaOptions)
        }
        func resolveUrl(_ src: String) -> String { rewrite[src] ?? src }
    }

    func testDeniedSrcsLoadNothing() throws {
        // The default plugin goes through the media policy (no raw fallback).
        let bare = NoHost()
        XCTAssertNil(bare.mediaRequest("file:///etc/passwd"))
        XCTAssertNil(bare.mediaRequest("javascript:alert(3)"))
        XCTAssertNil(bare.mediaRequest("/no/base.png"))
        XCTAssertEqual(bare.mediaRequest("https://cdn.example/a.png")?.url?.absoluteString, "https://cdn.example/a.png")
        XCTAssertNotNil(bare.mediaRequest("data:image/png;base64,iVBORw0KGgo="))

        // schemes / hosts.
        XCTAssertNil(policyMediaRequest("https://evil.example/a.png", options: MediaOptions(hosts: ["*.exponential.at"])))
        XCTAssertNotNil(policyMediaRequest("https://cdn.exponential.at/a.png", options: MediaOptions(hosts: ["*.exponential.at"])))
        XCTAssertNotNil(policyMediaRequest("file:///tmp/a.png", options: MediaOptions(schemes: ["file"])))

        // resolveUrl only REWRITES before the policy.
        let plugin = RawPlugin()
        let m = try SurfaceModel(id: "m", options: SurfaceOptions(), host: plugin)
        plugin.rewrite = ["att:1": "https://app.exponential.at/api/attachments/1", "att:2": "file:///etc/hosts"]
        XCTAssertEqual(m.mediaRequest("att:1")?.url?.absoluteString, "https://app.exponential.at/api/attachments/1")
        XCTAssertNil(m.mediaRequest("att:2"))
        // A hook that builds a raw request is re-checked against the schemes.
        plugin.raw = "file:///etc/passwd"
        XCTAssertNil(m.mediaRequest("anything"))
        plugin.mediaOptions = MediaOptions(schemes: ["file"])
        XCTAssertNotNil(m.mediaRequest("anything"), "the host listed file")

        // ExponentialHost: media hosts hold through the bridge.
        let host = ExponentialHost(HostOptions(policy: HostPolicy(media: MediaOptions(baseUrl: "https://app.exponential.at", hosts: ["app.exponential.at"]))))
        XCTAssertNotNil(host.plugin.mediaRequest("/api/attachments/a"))
        XCTAssertNil(host.plugin.mediaRequest("https://cdn.example/x.png"))
        XCTAssertNil(host.plugin.mediaRequest("javascript:alert(1)"))
    }

    // MARK: - media.limits

    func testTheMediaLimitsComeFromTheContract() {
        XCTAssertEqual(MediaLimits.contract, MediaLimits(maxBytes: 20_971_520, timeoutMs: 30_000, maxPixels: 33_554_432))
        XCTAssertEqual(MediaLoader.shared.limits, .contract)
    }

    /// A PNG header (signature + IHDR with its CRC) claiming `w`×`h`.
    static func pngHeader(_ w: UInt32, _ h: UInt32) -> Data {
        var ihdr = Data("IHDR".utf8)
        for v in [w, h] { withUnsafeBytes(of: v.bigEndian) { ihdr.append(contentsOf: $0) } }
        ihdr.append(contentsOf: [8, 6, 0, 0, 0])
        var data = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13])
        data.append(ihdr)
        withUnsafeBytes(of: crc32(ihdr).bigEndian) { data.append(contentsOf: $0) }
        return data
    }

    static func crc32(_ data: Data) -> UInt32 {
        var c: UInt32 = 0xFFFF_FFFF
        for b in data {
            c ^= UInt32(b)
            for _ in 0..<8 { c = (c & 1) != 0 ? (c >> 1) ^ 0xEDB8_8320 : c >> 1 }
        }
        return ~c
    }

    static func realPng(_ w: Int, _ h: Int) -> Data {
        let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        ctx.setFillColor(CGColor(red: 1, green: 0, blue: 0, alpha: 1))
        ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
        let out = NSMutableData()
        let dest = CGImageDestinationCreateWithData(out, UTType.png.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(dest, ctx.makeImage()!, nil)
        CGImageDestinationFinalize(dest)
        return out as Data
    }

    func testAPictureOverMaxPixelsIsRefusedBeforeDecoding() {
        let bomb = Self.pngHeader(40_000, 30_000)
        XCTAssertEqual(MediaLoader.pixelSize(bomb).map { [$0.0, $0.1] }, [40_000, 30_000], "read from the header")
        guard case .failure(.tooManyPixels(40_000, 30_000)) = MediaLoader.decode(bomb, limits: .contract) else { return XCTFail("decoded a 1.2 Gpx header") }
        guard case .success(let image) = MediaLoader.decode(Self.realPng(8, 4), limits: .contract) else { return XCTFail("a small png decodes") }
        XCTAssertEqual(image.size.width, 8)
        guard case .failure(.undecodable) = MediaLoader.decode(Data("<svg/>".utf8), limits: .contract) else { return XCTFail("no header, no decode") }
        var tight = MediaLimits.contract
        tight.maxBytes = 10
        guard case .failure(.tooLarge) = MediaLoader.decode(Self.realPng(8, 4), limits: tight) else { return XCTFail("byte cap") }
    }

    /// Answers every request with `StubProtocol.reply` (nil = never
    /// answers) after `delay` seconds; a url in `redirects` answers a 302 to
    /// its Location.
    final class StubProtocol: URLProtocol {
        nonisolated(unsafe) static var reply: (headers: [String: String], body: Data)?
        nonisolated(unsafe) static var redirects: [String: String] = [:]
        nonisolated(unsafe) static var delay: TimeInterval = 0
        /// Every request that reached the network.
        nonisolated(unsafe) static var seen: [URLRequest] = []
        override class func canInit(with request: URLRequest) -> Bool { true }
        override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
        override func startLoading() {
            Self.seen.append(request)
            if let location = Self.redirects[request.url!.absoluteString] {
                let response = HTTPURLResponse(url: request.url!, statusCode: 302, httpVersion: "HTTP/1.1", headerFields: ["Location": location])!
                var next = request
                next.url = URL(string: location)
                client?.urlProtocol(self, wasRedirectedTo: next, redirectResponse: response)
                return
            }
            guard let reply = Self.reply else { return }
            let answer = { [self] in
                let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: reply.headers)!
                client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
                client?.urlProtocol(self, didLoad: reply.body)
                client?.urlProtocolDidFinishLoading(self)
            }
            if Self.delay > 0 { DispatchQueue.global().asyncAfter(deadline: .now() + Self.delay, execute: answer) } else { answer() }
        }
        override func stopLoading() {}
    }

    func stubSession() -> URLSession {
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [StubProtocol.self]
        return URLSession(configuration: config)
    }

    func testTheLoaderEnforcesContentLengthPixelsAndTheTimeout() async {
        let session = stubSession()
        let request = URLRequest(url: URL(string: "https://cdn.example/a.png")!)
        // A Content-Length over maxBytes is refused up front.
        StubProtocol.reply = (["Content-Length": "\(30 * 1024 * 1024)", "Content-Type": "image/png"], Self.realPng(2, 2))
        let big = await MediaLoader.fetch(request, session: session, limits: .contract)
        guard case .failure(.tooLarge) = big else { return XCTFail("oversize Content-Length: \(big)") }
        // A header claiming 40000×30000 never decodes.
        StubProtocol.reply = (["Content-Type": "image/png"], Self.pngHeader(40_000, 30_000))
        let bomb = await MediaLoader.fetch(request, session: session, limits: .contract)
        guard case .failure(.tooManyPixels) = bomb else { return XCTFail("pixel bomb: \(bomb)") }
        // A body that streams past maxBytes is cut off.
        var small = MediaLimits.contract
        small.maxBytes = 64
        StubProtocol.reply = (["Content-Type": "image/png"], Data(count: 200 * 1024))
        let streamed = await MediaLoader.fetch(request, session: session, limits: small)
        guard case .failure(.tooLarge) = streamed else { return XCTFail("streamed body: \(streamed)") }
        // The whole request is bounded by timeoutMs.
        StubProtocol.reply = nil
        var quick = MediaLimits.contract
        quick.timeoutMs = 100
        let t0 = Date()
        let hung = await MediaLoader.fetch(request, session: session, limits: quick)
        guard case .failure(.timedOut) = hung else { return XCTFail("timeout: \(hung)") }
        XCTAssertLessThan(Date().timeIntervalSince(t0), 5)
        // And a fitting picture loads.
        StubProtocol.reply = (["Content-Type": "image/png"], Self.realPng(3, 3))
        let ok = await MediaLoader.fetch(request, session: session, limits: .contract)
        guard case .success = ok else { return XCTFail("small png: \(ok)") }
    }

    // MARK: - Video / AudioPlayer src

    /// Records every src the painter hands the media policy.
    final class MediaLog: HostPlugin {
        var asked: [String] = []
        var mediaOptions: MediaOptions? {
            MediaOptions(baseUrl: "https://app.exponential.at", rules: [.init(prefix: "https://app.exponential.at/api/", headers: ["authorization": "Bearer expu_test"])], hosts: ["app.exponential.at"])
        }
        func mediaRequest(_ src: String) -> URLRequest? {
            asked.append(src)
            return policyMediaRequest(resolveUrl(src), options: mediaOptions)
        }
    }

    func testVideoAndAudioSrcsPlayThroughTheMediaPolicy() async throws {
        let plugin = MediaLog()
        let m = try SurfaceModel(id: "av", options: SurfaceOptions(), host: plugin)
        m.fixedMeasure = true
        let tree = JSONValue.object(["id": .string("root"), "component": .string("Stack"), "props": .object([:]), "children": .array([
            .object(["id": .string("video"), "component": .string("Video"), "props": .object(["src": .string("/api/attachments/v1"), "durationMs": .number(12000)])]),
            .object(["id": .string("audio"), "component": .string("AudioPlayer"), "props": .object(["src": .string("https://cdn.example/track.mp3"), "title": .string("Episode 12")])]),
        ])])
        try m.setNested(json: tree.json)
        paint(m)
        // The painters ask the media policy for each src (not only the poster).
        XCTAssertTrue(plugin.asked.contains("/api/attachments/v1"), "\(plugin.asked)")
        XCTAssertTrue(plugin.asked.contains("https://cdn.example/track.mp3"), "\(plugin.asked)")

        let saved = MediaLoader.shared.session
        MediaLoader.shared.session = stubSession()
        defer { MediaLoader.shared.session = saved }

        // Allowed: the policed request (resolved, with the rule's header)
        // STREAMS with that header (no byte cap on Video / Audio): a policed
        // probe resolves it, the player opens it with the header.
        let allowed = try XCTUnwrap(m.mediaRequest("/api/attachments/v1"))
        XCTAssertEqual(MediaLoader.playback(allowed), .stream(allowed))
        StubProtocol.seen = []
        StubProtocol.reply = (["Content-Type": "video/mp4", "Content-Length": "\(30 * 1024 * 1024)"], Data(repeating: 0, count: 1))
        let playback = MediaPlayback(kind: .video)
        await playback.play(allowed, options: m.mediaPolicy)
        XCTAssertEqual(StubProtocol.seen.map { $0.url?.absoluteString }, ["https://app.exponential.at/api/attachments/v1"])
        XCTAssertEqual(StubProtocol.seen.first?.value(forHTTPHeaderField: "authorization"), "Bearer expu_test")
        XCTAssertEqual(StubProtocol.seen.first?.value(forHTTPHeaderField: "range"), "bytes=0-0")
        let asset = try XCTUnwrap(playback.player?.currentItem?.asset as? AVURLAsset)
        XCTAssertEqual(asset.url.absoluteString, "https://app.exponential.at/api/attachments/v1", "over 20 MB still plays: streamed, not fetched")
        playback.stop()
        XCTAssertNil(playback.player)

        // An allowed src without headers streams too.
        let plain = try XCTUnwrap(m.mediaRequest("https://app.exponential.at/clip.mp4"))
        XCTAssertEqual(MediaLoader.playback(plain), .stream(plain))
        // A data: src plays from a file.
        let data = try XCTUnwrap(m.mediaRequest("data:audio/mpeg;base64,SUQz"))
        XCTAssertEqual(MediaLoader.playback(data), .file(data))

        // Denied: no request, so nothing loads and no player opens.
        XCTAssertNil(m.mediaRequest("https://cdn.example/track.mp3"))
        XCTAssertNil(m.mediaRequest("file:///etc/passwd"))
        StubProtocol.seen = []
        await playback.play(m.mediaRequest("https://cdn.example/track.mp3"))
        XCTAssertNil(playback.player)
        XCTAssertTrue(StubProtocol.seen.isEmpty)
    }

    // MARK: - onPaintError

    func testPaintErrorsBecomeOneRenderFailedPerComponent() async throws {
        let transport = MemoryTransport()
        let host = ExponentialHost(HostOptions(transport: transport))
        host.connect()
        host.receive(msg("createSurface", .object(["surfaceId": .string("s1"), "catalogId": .string(coreCatalogId())])))
        let e = SurfacePaintError(surfaceId: "s1", componentId: "chart", message: "bad series")
        host.paintError(e)
        host.paintError(e)
        // Keyed by component, not message.
        host.paintError(SurfacePaintError(surfaceId: "s1", componentId: "chart", message: "other"))
        host.paintError(SurfacePaintError(surfaceId: "s1", componentId: "table", message: "other"))
        await settle()
        let errors = transport.sentMessages.compactMap { $0["error"] }
        XCTAssertEqual(errors.count, 2)
        XCTAssertEqual(errors.first?["code"], .string("RENDER_FAILED"))
        XCTAssertEqual(errors.first?["surfaceId"], .string("s1"))
        XCTAssertEqual(errors.first?["message"], .string("bad series"))
        XCTAssertEqual(errors.first?["path"], .string("/components/chart"))
        // An update of OTHER components keeps chart's report...
        host.receive(msg("updateComponents", .object(["surfaceId": .string("s1"), "components": .array([.object(["id": .string("root"), "component": .string("Text"), "text": .string("x")])])])))
        host.paintError(e)
        await settle()
        XCTAssertEqual(transport.sentMessages.compactMap { $0["error"] }.count, 2)
        // ... one naming chart clears it (per node).
        host.receive(msg("updateComponents", .object(["surfaceId": .string("s1"), "components": .array([.object(["id": .string("chart"), "component": .string("Text"), "text": .string("y")])])])))
        host.paintError(e)
        host.paintError(SurfacePaintError(surfaceId: "s1", componentId: "table", message: "other"))
        await settle()
        XCTAssertEqual(transport.sentMessages.compactMap { $0["error"] }.count, 3)
        XCTAssertEqual(transport.sentMessages.compactMap { $0["error"] }.last?["path"], .string("/components/chart"))
        // The bridge routes a model's report to the host.
        host.plugin.onPaintError(SurfacePaintError(surfaceId: "s1", componentId: "c2", message: "m"))
        await settle()
        XCTAssertEqual(transport.sentMessages.compactMap { $0["error"] }.last?["path"], .string("/components/c2"))
    }

    final class ThrowingPainter: ExtensionPainter {
        struct Boom: Error, CustomStringConvertible {
            var description: String { "boom" }
        }
        func measure(_ leaf: ExtensionLeaf, wrap: CGFloat?) -> CGSize? { CGSize(width: 120, height: 40) }
        var calls = 0
        func paint(_ context: ExtensionContext) throws -> AnyView {
            calls += 1
            throw Boom()
        }
    }

    func testAThrowingPainterPaintsAnEmptyBoxAndReportsOnce() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
        let fixture = try Fixtures.json("catalog-extension.json")
        let definition = fixture["extension"]!.json
        let c = try XCTUnwrap(fixture["cases"]?.array?.first { $0["expected"]?["root"]?["component"]?.string == "TrendLine" })
        var reports: [SurfacePaintError] = []
        let plugin = ClosureHost(paintErrors: { reports.append($0) })
        var options = SurfaceOptions()
        options.catalogId = c["catalogId"]?.string ?? coreCatalogId()
        let m = try SurfaceModel(id: "ext", options: options, host: plugin)
        let painter = ThrowingPainter()
        let kinds = ["TrendLine", "StatCard"]
        try m.register(extension: definition, painters: Dictionary(uniqueKeysWithValues: kinds.map { ($0, painter as ExtensionPainter) }))
        m.setViewport(width: 390, height: 600)
        try m.setComponents(json: c["components"]!.json)
        let leaves = m.nodes.filter { $0.component == "Extension" }
        XCTAssertFalse(leaves.isEmpty)
        paint(m)
        let calls = painter.calls
        paint(m)
        await settle()
        XCTAssertEqual(Set(reports.map(\.componentId)), Set(leaves.map(\.id)), "every failing leaf reported")
        XCTAssertEqual(reports.count, leaves.count, "once per component")
        XCTAssertTrue(reports.allSatisfy { $0.surfaceId == "ext" && $0.message.contains("boom") })
        XCTAssertEqual(painter.calls, calls, "a failed node is not painted again on the same props")
        // New props on ONE node clear its failure only: it paints (and
        // reports) again, the others stay failed.
        let first = try XCTUnwrap(leaves.first)
        XCTAssertTrue(m.paintFailed(componentId: first.id, props: m.node(first.index)!.props.json))
        XCTAssertFalse(m.paintFailed(componentId: first.id, props: "{\"changed\":true}"))
        for other in leaves.dropFirst() { XCTAssertTrue(m.paintFailed(componentId: other.id, props: m.node(other.index)!.props.json)) }
        m.paintError(componentId: first.id, message: "boom again", props: "{\"changed\":true}")
        m.paintError(componentId: first.id, message: "boom again", props: "{\"changed\":true}")
        await settle()
        XCTAssertEqual(reports.count, leaves.count + 1)
        // A full replacement forgets every failure.
        try m.setComponents(json: c["components"]!.json)
        XCTAssertTrue(m.paintFailures.isEmpty)
    }

    func testASelectWithNullOptionsSkipsThem() throws {
        var o = SurfaceOptions()
        o.theme = ThemeHandle.builtin("exponential")
        let m = try SurfaceModel(id: "sel", options: o, host: NoHost())
        try m.setComponents(json: JSONValue.array([.object([
            "id": .string("root"), "component": .string("Select"), "label": .string("Pick"),
            "options": .array([.null, .object(["value": .string("a"), "label": .string("A")]), .null]),
        ])]).json)
        m.setViewport(width: 390, height: 600)
        m.setOpen("root", true)
        paint(m)
        let items = m.nodes.filter { !$0.removed && $0.recipeComponent == "Select" && $0.part == "item" }
        // Painted without a crash; the real option is there. (The core
        // still lays out an empty row per null entry.)
        XCTAssertEqual(items.filter { $0.props.str("value") == "a" }.count, 1)
    }

    // MARK: - markdown

    func testMarkdownLinksAndImagesPassThePolicy() throws {
        var o = SurfaceOptions()
        o.theme = ThemeHandle.builtin("exponential")
        let m = try SurfaceModel(id: "md", options: o, host: NoHost())
        let text = "See [docs](https://x/A_(b)) and [bad](javascript:alert(1)).\n\n![denied](javascript:alert(3))\n\n![ok](https://cdn.example/p.png)\n\n- a\n  - b\n    1. c\n- [x] d"
        try m.setComponents(json: JSONValue.array([.object(["id": .string("root"), "component": .string("Markdown"), "text": .string(text)])]).json)
        let blocks = MarkdownPainter.blocks(text, model: m)
        // The denied image is its alt text; the allowed one keeps its box.
        XCTAssertEqual(blocks[1].kind, .paragraph)
        XCTAssertEqual(Markdown.plain(blocks[1].inlines), "denied")
        XCTAssertEqual(blocks[2].kind, .image(src: "https://cdn.example/p.png", alt: "ok"))
        XCTAssertEqual(blocks[3...].map(\.depth), [0, 1, 2, 0])
        // Links: the allowed href stays, the denied one is plain text.
        let links = Markdown.mapLinks(blocks, m.linkHref)[0].inlines.compactMap(\.link)
        XCTAssertEqual(links, ["https://x/A_(b)"])
        // The measurer and the painter agree on the height.
        m.setViewport(width: 320, height: 900)
        paint(m, width: 320)
        let root = try XCTUnwrap(m.nodes.first { $0.id == "root" })
        let styles = MarkdownPainter.styles(theme: m.theme, mode: m.mode, body: m.textStyle(root.index), props: root.props)
        let laid = Markdown.layout(blocks, styles, width: m.frame(root.index).width, text: MarkdownShaper(mono: m.theme?.monoFamily))
        XCTAssertEqual(m.frame(root.index).height, laid.height, accuracy: 1)
        XCTAssertGreaterThan(laid.height, styles.imageHeight)
    }

    func testMarkdownLinkHitTesting() {
        let spec = MarkdownTextSpec(size: 14, lineHeight: 20, weight: 400, family: nil)
        let runs = TextShaper.runs(Markdown.parseInline("go [here](https://x.y) now"), spec, mono: nil, ink: .black, link: .blue, codeBackground: nil)
        XCTAssertTrue(TextShaper.hasLinks(runs))
        let prefix = TextShaper.lineWidth(NSAttributedString(attributedString: runs.attributedSubstring(from: NSRange(location: 0, length: 3))))
        XCTAssertEqual(TextShaper.link(in: runs, lineHeight: 20, wrap: 400, at: CGPoint(x: prefix + 6, y: 10)), "https://x.y")
        XCTAssertNil(TextShaper.link(in: runs, lineHeight: 20, wrap: 400, at: CGPoint(x: 2, y: 10)))
        XCTAssertNil(TextShaper.link(in: runs, lineHeight: 20, wrap: 400, at: CGPoint(x: prefix + 6, y: 30)))
    }
}
