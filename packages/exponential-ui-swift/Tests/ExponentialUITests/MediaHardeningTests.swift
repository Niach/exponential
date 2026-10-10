import XCTest
import AVFoundation
import ExponentialUICore
@testable import ExponentialUI

/// VAPP-103 round 4 (NFIX): every redirect hop of a media load is policed,
/// one open per playback, fetched media files are bounded and deleted, an
/// authed Video / Audio streams (no byte cap), the temporary file's type.
@MainActor
final class MediaHardeningTests: XCTestCase {
    typealias Stub = SafeHostTests.StubProtocol

    let options = MediaOptions(
        rules: [.init(prefix: "https://app.exponential.at/api/", headers: ["authorization": "Bearer expu_test"])],
        hosts: ["app.exponential.at", "cdn.exponential.at"]
    )

    func session() -> URLSession {
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [Stub.self]
        return URLSession(configuration: config)
    }

    override func setUp() async throws {
        Stub.seen = []
        Stub.redirects = [:]
        Stub.delay = 0
        Stub.reply = (["Content-Type": "image/png"], SafeHostTests.realPng(2, 2))
    }

    override func tearDown() async throws {
        Stub.redirects = [:]
        Stub.delay = 0
    }

    func request(_ url: String) -> URLRequest { policyMediaRequest(url, options: options)! }

    // MARK: - Redirects

    func testARedirectHopIsPolicedLikeTheSrc() {
        let from = request("https://app.exponential.at/api/attachments/a")
        XCTAssertEqual(from.value(forHTTPHeaderField: "authorization"), "Bearer expu_test")
        // A host the policy does not list.
        XCTAssertNil(MediaLoader.policeRedirect(from: from, to: URLRequest(url: URL(string: "https://evil.example/a.png")!), options: options))
        // A scheme it does not list.
        XCTAssertNil(MediaLoader.policeRedirect(from: from, to: URLRequest(url: URL(string: "file:///etc/passwd")!), options: options))
        // https → http is a downgrade, even to an allowed host.
        XCTAssertNil(MediaLoader.policeRedirect(from: from, to: URLRequest(url: URL(string: "http://app.exponential.at/api/attachments/a")!), options: options))
        // An allowed url outside the rule loses the rule's header (the
        // original request's headers never carry over).
        var carried = URLRequest(url: URL(string: "https://cdn.exponential.at/a.png")!)
        carried.setValue("Bearer expu_test", forHTTPHeaderField: "authorization")
        let off = MediaLoader.policeRedirect(from: from, to: carried, options: options)
        XCTAssertEqual(off?.url?.absoluteString, "https://cdn.exponential.at/a.png")
        XCTAssertNil(off?.value(forHTTPHeaderField: "authorization"))
        // One inside the rule keeps it; a probe's Range carries over.
        var probe = from
        probe.setValue("bytes=0-0", forHTTPHeaderField: "Range")
        let on = MediaLoader.policeRedirect(from: probe, to: URLRequest(url: URL(string: "https://app.exponential.at/api/attachments/b")!), options: options, keep: ["Range"])
        XCTAssertEqual(on?.value(forHTTPHeaderField: "authorization"), "Bearer expu_test")
        XCTAssertEqual(on?.value(forHTTPHeaderField: "range"), "bytes=0-0")
        // http → http is no downgrade.
        let plain = MediaOptions(hosts: ["a.example", "b.example"])
        XCTAssertNotNil(MediaLoader.policeRedirect(from: URLRequest(url: URL(string: "http://a.example/x")!), to: URLRequest(url: URL(string: "http://b.example/y")!), options: plain))
    }

    func testA302ToADeniedHostFailsTheLoad() async {
        Stub.redirects = ["https://app.exponential.at/api/attachments/a": "https://evil.example/a.png"]
        let got = await MediaLoader.fetch(request("https://app.exponential.at/api/attachments/a"), session: session(), limits: .contract, options: options)
        guard case .failure(.denied) = got else { return XCTFail("followed a denied redirect: \(got)") }
        XCTAssertEqual(Stub.seen.map { $0.url?.host }, ["app.exponential.at"], "the denied host was never asked")
    }

    func testA302DowngradeFailsTheLoad() async {
        Stub.redirects = ["https://app.exponential.at/api/attachments/a": "http://app.exponential.at/api/attachments/a"]
        let got = await MediaLoader.fetch(request("https://app.exponential.at/api/attachments/a"), session: session(), limits: .contract, options: options)
        guard case .failure(.denied) = got else { return XCTFail("followed an https→http redirect: \(got)") }
        XCTAssertEqual(Stub.seen.count, 1)
    }

    func testA302OutsideTheRuleDropsItsHeader() async {
        Stub.redirects = ["https://app.exponential.at/api/attachments/a": "https://cdn.exponential.at/a.png"]
        let got = await MediaLoader.fetch(request("https://app.exponential.at/api/attachments/a"), session: session(), limits: .contract, options: options)
        guard case .success = got else { return XCTFail("an allowed redirect: \(got)") }
        XCTAssertEqual(Stub.seen.map { $0.url?.absoluteString }, ["https://app.exponential.at/api/attachments/a", "https://cdn.exponential.at/a.png"])
        XCTAssertEqual(Stub.seen.first?.value(forHTTPHeaderField: "authorization"), "Bearer expu_test")
        XCTAssertNil(Stub.seen.last?.value(forHTTPHeaderField: "authorization"), "the auth header left with the rule")
    }

    func testAStreamResolvesItsRedirectsBeforeThePlayerOpens() async {
        Stub.redirects = ["https://app.exponential.at/api/attachments/v": "https://cdn.exponential.at/v.mp4"]
        Stub.reply = (["Content-Type": "video/mp4"], Data(count: 1))
        let got = await MediaLoader.resolveStream(request("https://app.exponential.at/api/attachments/v"), session: session(), limits: .contract, options: options)
        XCTAssertEqual(got?.url.absoluteString, "https://cdn.exponential.at/v.mp4")
        XCTAssertEqual(got?.headers, [:], "the final hop's headers: the rule no longer matches")
        Stub.redirects = ["https://app.exponential.at/api/attachments/v": "https://evil.example/v.mp4"]
        let denied = await MediaLoader.resolveStream(request("https://app.exponential.at/api/attachments/v"), session: session(), limits: .contract, options: options)
        XCTAssertNil(denied)
    }

    // MARK: - One open per playback

    func testASecondPlayDuringTheOpenBuildsNoSecondPlayer() async throws {
        let saved = MediaLoader.shared.session
        MediaLoader.shared.session = session()
        defer { MediaLoader.shared.session = saved }
        Stub.reply = (["Content-Type": "video/mp4"], Data(count: 1))
        Stub.delay = 0.3
        let r = request("https://app.exponential.at/api/attachments/v1")
        let playback = MediaPlayback(kind: .video)
        let first = Task { await playback.play(r, options: options) }
        try await Task.sleep(nanoseconds: 50_000_000)
        XCTAssertTrue(playback.loading, "the button waits while it opens")
        await playback.play(r, options: options)
        await first.value
        XCTAssertFalse(playback.loading)
        XCTAssertEqual(playback.playersBuilt, 1)
        XCTAssertEqual(Stub.seen.count, 1, "one probe")
        let firstPlayer = try XCTUnwrap(playback.player)
        XCTAssertTrue(playback.observing)

        // Another request replaces the player (its observers go first).
        Stub.delay = 0
        await playback.play(request("https://app.exponential.at/api/attachments/v2"), options: options)
        XCTAssertEqual(playback.playersBuilt, 2)
        XCTAssertFalse(playback.player === firstPlayer)
        XCTAssertNil(firstPlayer.currentItem, "the replaced player let its item go")
        XCTAssertTrue(playback.observing)
        playback.stop()
        XCTAssertFalse(playback.observing)
        XCTAssertNil(playback.player)
    }

    func testAPlayForAnotherRequestCancelsTheOpenInFlight() async throws {
        let saved = MediaLoader.shared.session
        MediaLoader.shared.session = session()
        defer { MediaLoader.shared.session = saved }
        Stub.reply = (["Content-Type": "video/mp4"], Data(count: 1))
        Stub.delay = 0.3
        let playback = MediaPlayback(kind: .video)
        let first = Task { await playback.play(request("https://app.exponential.at/api/attachments/slow"), options: options) }
        try await Task.sleep(nanoseconds: 50_000_000)
        Stub.delay = 0
        await playback.play(request("https://app.exponential.at/api/attachments/fast"), options: options)
        await first.value
        XCTAssertEqual(playback.playersBuilt, 1)
        let asset = try XCTUnwrap(playback.player?.currentItem?.asset as? AVURLAsset)
        XCTAssertEqual(asset.url.lastPathComponent, "fast")
        playback.stop()
    }

    // MARK: - Fetched files

    func dataRequest(_ base64: String, mime: String = "audio/mpeg") -> URLRequest {
        URLRequest(url: URL(string: "data:\(mime);base64,\(base64)")!)
    }

    func testFetchedFilesAreDeletedWhenNoPlayerHoldsThem() async throws {
        let loader = MediaLoader.shared
        // The first fetch of the process empties the media directory.
        loader.clearedDirectory = false
        try FileManager.default.createDirectory(at: MediaLoader.mediaDirectory, withIntermediateDirectories: true)
        let stale = MediaLoader.mediaDirectory.appendingPathComponent("stale.mp4")
        try Data([1]).write(to: stale)
        let a = try await XCTUnwrapAsync(await loader.playable(dataRequest("SUQzAAAA"), kind: .audio, options: nil))
        XCTAssertFalse(FileManager.default.fileExists(atPath: stale.path), "a previous process's file went")
        XCTAssertTrue(a.url.isFileURL)
        XCTAssertEqual(a.url.pathExtension, "mp3")
        // Shared: the same request reuses the file; it stays while either holds it.
        let b = try await XCTUnwrapAsync(await loader.playable(dataRequest("SUQzAAAA"), kind: .audio, options: nil))
        XCTAssertEqual(a.url, b.url)
        loader.release(a)
        XCTAssertTrue(FileManager.default.fileExists(atPath: b.url.path))
        loader.release(b)
        XCTAssertFalse(FileManager.default.fileExists(atPath: b.url.path), "deleted once nobody plays it")

        // A cached file deleted behind the loader's back is fetched again.
        let c = try await XCTUnwrapAsync(await loader.playable(dataRequest("SUQzAAAB"), kind: .audio, options: nil))
        try FileManager.default.removeItem(at: c.url)
        let d = try await XCTUnwrapAsync(await loader.playable(dataRequest("SUQzAAAB"), kind: .audio, options: nil))
        XCTAssertNotEqual(c.url, d.url)
        XCTAssertTrue(FileManager.default.fileExists(atPath: d.url.path))
        loader.release(c)
        loader.release(d)
    }

    func testTheFileCacheIsBounded() async throws {
        let loader = MediaLoader.shared
        let saved = loader.maxFiles
        loader.maxFiles = 2
        defer { loader.maxFiles = saved }
        var held: [MediaLoader.Playable] = []
        for i in 0..<4 {
            held.append(try await XCTUnwrapAsync(await loader.playable(dataRequest(Data("ID3\(i)".utf8).base64EncodedString()), kind: .audio, options: nil)))
        }
        // Files a player holds are never evicted...
        XCTAssertTrue(held.allSatisfy { FileManager.default.fileExists(atPath: $0.url.path) })
        held.forEach { loader.release($0) }
        // ... and once released none is left over.
        XCTAssertTrue(held.allSatisfy { !FileManager.default.fileExists(atPath: $0.url.path) })
        XCTAssertTrue(loader.fileURLs.allSatisfy { url in !held.contains { $0.url == url } })
    }

    func testStopDeletesTheFileOfADataSrc() async throws {
        let playback = MediaPlayback(kind: .audio)
        await playback.play(dataRequest("SUQzAAAC"))
        let asset = try XCTUnwrap(playback.player?.currentItem?.asset as? AVURLAsset)
        XCTAssertTrue(asset.url.isFileURL)
        XCTAssertTrue(FileManager.default.fileExists(atPath: asset.url.path))
        playback.stop()
        XCTAssertFalse(FileManager.default.fileExists(atPath: asset.url.path))
    }

    // MARK: - File types

    func testTheTemporaryFileTypeFollowsTheMimeTheBytesThenTheComponent() {
        let id3 = Data("ID3\u{3}\u{0}".utf8)
        XCTAssertEqual(MediaLoader.fileExtension(mime: "audio/mpeg", data: Data(), kind: .audio), "mp3")
        XCTAssertEqual(MediaLoader.fileExtension(mime: "audio/mp4; codecs=mp4a", data: Data(), kind: .audio), "m4a")
        XCTAssertEqual(MediaLoader.fileExtension(mime: "video/quicktime", data: Data(), kind: .video), "mov")
        // octet-stream: sniffed.
        XCTAssertEqual(MediaLoader.fileExtension(mime: "application/octet-stream", data: id3, kind: .audio), "mp3")
        XCTAssertEqual(MediaLoader.fileExtension(mime: "application/octet-stream", data: Data([0xFF, 0xFB, 0x90, 0x00]), kind: .audio), "mp3")
        XCTAssertEqual(MediaLoader.fileExtension(mime: nil, data: Data("RIFF\u{0}\u{0}\u{0}\u{0}WAVEfmt ".utf8), kind: .audio), "wav")
        XCTAssertEqual(MediaLoader.fileExtension(mime: nil, data: Data("\u{0}\u{0}\u{0}\u{20}ftypM4A ".utf8), kind: .audio), "m4a")
        XCTAssertEqual(MediaLoader.fileExtension(mime: nil, data: Data("\u{0}\u{0}\u{0}\u{20}ftypisom".utf8), kind: .video), "mp4")
        XCTAssertEqual(MediaLoader.fileExtension(mime: nil, data: Data("\u{0}\u{0}\u{0}\u{20}ftypqt  ".utf8), kind: .video), "mov")
        XCTAssertEqual(MediaLoader.fileExtension(mime: nil, data: Data("fLaC".utf8), kind: .audio), "flac")
        // Unknown bytes: by the component, never mp4 for an AudioPlayer.
        XCTAssertEqual(MediaLoader.fileExtension(mime: "application/octet-stream", data: Data([1, 2, 3]), kind: .audio), "m4a")
        XCTAssertEqual(MediaLoader.fileExtension(mime: "application/octet-stream", data: Data([1, 2, 3]), kind: .video), "mp4")
    }
}

func XCTUnwrapAsync<T>(_ value: @autoclosure () async throws -> T?, file: StaticString = #filePath, line: UInt = #line) async throws -> T {
    let v = try await value()
    return try XCTUnwrap(v, file: file, line: line)
}
