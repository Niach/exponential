import XCTest
import Foundation
@testable import ExponentialUI

// VAPP-271 (catalog/host.json media.appFile): an `app-file:<fileId>` src
// loads through the host's `appFile` hook (the Engine's runners: their
// BlobStore) under the image limits, cached like any image; a host without
// one loads nothing, so the missing-image fallback paints.
@MainActor
final class AppFileTests: XCTestCase {
    /// fixtures/host-policy.json `appFiles`: appFileId(src) → the id | null.
    func testAppFilesFixture() throws {
        let cases = try Fixtures.json("host-policy.json")["appFiles"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let want = c["expected"]?.string
            XCTAssertEqual(appFileId(c["src"]!.string!), want, c["name"]?.string ?? "")
        }
    }

    /// Counts the hook's asks (from any thread).
    final class Asks: @unchecked Sendable {
        private let lock = NSLock()
        private var ids: [String] = []
        func add(_ id: String) { lock.withLock { ids.append(id) } }
        var all: [String] { lock.withLock { ids } }
    }

    func testAHostWithoutAppFilesLoadsNothing() async throws {
        let m = try SurfaceModel(id: "m", options: SurfaceOptions(), host: NoHost())
        XCTAssertNil(m.mediaRequest("app-file:blob-1"), "no hook: the fallback")
        // The media policy alone never loads the form either.
        XCTAssertNil(policyMediaRequest("app-file:blob-1", options: nil))
        let none = await MediaLoader.shared.load(MediaLoader.appFileRequest("blob-none")!, options: nil, appFile: nil)
        XCTAssertNil(none)
    }

    func testTheHookAnswersTheBytesOnceAndTheImageIsCached() async throws {
        let png = SafeHostTests.realPng(3, 2)
        let asks = Asks()
        let host = ClosureHost(appFile: { id in asks.add(id); return id == "blob-cached" ? png : nil })
        let m = try SurfaceModel(id: "m", options: SurfaceOptions(), host: host)
        let request = try XCTUnwrap(m.mediaRequest("app-file:blob-cached"))
        XCTAssertEqual(request.url?.absoluteString, "app-file:blob-cached")
        XCTAssertNil(m.mediaRequest("app-file:a/b"), "not an id: the media policy, which denies the scheme")

        let image = await MediaLoader.shared.load(request, options: m.mediaPolicy, appFile: m.appFile)
        XCTAssertNotNil(image)
        XCTAssertEqual(image?.size.width, 3)
        XCTAssertNotNil(MediaLoader.shared.cached(request))
        let again = await MediaLoader.shared.load(request, options: m.mediaPolicy, appFile: m.appFile)
        XCTAssertNotNil(again)
        XCTAssertEqual(asks.all, ["blob-cached"], "loaded once")

        // A missing file fails like a 404.
        let missing = await MediaLoader.shared.load(MediaLoader.appFileRequest("blob-gone")!, options: nil, appFile: m.appFile)
        XCTAssertNil(missing)
        // ExponentialHost forwards its policy's hook, else its plugin's.
        let eh = ExponentialHost(HostOptions(policy: HostPolicy(appFile: { _ in png })))
        XCTAssertNotNil(HostBridge(host: eh).appFile)
        let viaPlugin = ExponentialHost(HostOptions(plugin: host))
        XCTAssertNotNil(HostBridge(host: viaPlugin).appFile)
        XCTAssertNil(HostBridge(host: ExponentialHost(HostOptions())).appFile)
    }

    func testAnAppFileFitsTheImageLimits() async {
        let png = SafeHostTests.realPng(4, 4)
        let small = MediaLimits(maxBytes: 8, timeoutMs: 30_000, maxPixels: 1_000_000)
        let over = await MediaLoader.fetchAppFile("big", { _ in png }, limits: small)
        XCTAssertEqual(over.failureValue, .tooLarge(Int64(png.count)))
        let pixels = await MediaLoader.fetchAppFile("wide", { _ in png }, limits: MediaLimits(maxBytes: 1 << 20, timeoutMs: 30_000, maxPixels: 4))
        XCTAssertEqual(pixels.failureValue, .tooManyPixels(4, 4))
        let gone = await MediaLoader.fetchAppFile("gone", { _ in nil }, limits: .contract)
        XCTAssertEqual(gone.failureValue, .http(404))
        let junk = await MediaLoader.fetchAppFile("junk", { _ in Data([1, 2, 3]) }, limits: .contract)
        XCTAssertEqual(junk.failureValue, .undecodable)
    }

    func testAPlayerOpensNoAppFile() async {
        let got = await MediaLoader.shared.playable(MediaLoader.appFileRequest("clip")!, kind: .video, options: nil)
        XCTAssertNil(got)
    }
}

private extension Result {
    var failureValue: Failure? {
        if case let .failure(f) = self { return f }
        return nil
    }
}
