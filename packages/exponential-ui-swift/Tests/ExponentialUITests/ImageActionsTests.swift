import XCTest
import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives
@testable import ExponentialUI

/// #EXP-1926 / #EXP-1949 (image-actions.json): the host hooks a content
/// picture asks: `imageSource` (another website = a link in the picture's
/// place, never fetched; anything else refused = its alt), the image menu
/// on a RENDERED picture (`hasImageMenu` / `imageMenu`, with the original
/// bytes through the media policy), and an enclosing context Menu standing
/// down for a press a picture claimed (`pressMenu`).
@MainActor
final class ImageActionsTests: XCTestCase {
    /// The fixture's rule in miniature: ours (`/api/attachments/…`, `data:`)
    /// loads, `https://` elsewhere is a link, the rest is the alt.
    final class SourceHost: HostPlugin {
        var menus: [ImageTarget] = []
        var asked: [String] = []
        var urlPolicy: UrlPolicy? { UrlPolicy(schemes: ["https"], baseUrl: "https://app.example") }
        var mediaOptions: MediaOptions? { MediaOptions(baseUrl: "https://app.example", schemes: ["https", "data"]) }

        func imageSource(_ src: String, alt: String) -> ImageSource? {
            if src.hasPrefix("/api/attachments/") || src.hasPrefix("data:image/") { return .load }
            if src.hasPrefix("https://") || src.hasPrefix("http://") {
                let host = URL(string: src)?.host ?? ""
                return .link(href: src, text: alt.isEmpty ? "Image: \(host)" : alt)
            }
            return .text(alt)
        }

        func mediaRequest(_ src: String) -> URLRequest? {
            asked.append(src)
            return policyMediaRequest(resolveUrl(src), options: mediaOptions)
        }

        var hasImageMenu: Bool { true }

        func imageMenu(_ target: ImageTarget, onTap: (() -> Void)?, content: AnyView) -> AnyView {
            menus.append(target)
            return content
        }
    }

    private func model(_ host: HostPlugin) throws -> SurfaceModel {
        let m = try SurfaceModel(id: "img", options: SurfaceOptions(theme: ThemeHandle.builtin("exponential"), mode: .dark), host: host)
        m.fixedMeasure = true
        m.setViewport(width: 320, height: 800)
        return m
    }

    private func paint(_ m: SurfaceModel) {
        let root = ExponentialSurface(model: m).frame(width: 320)
        #if canImport(AppKit)
        let view = NSHostingView(rootView: root)
        view.frame = CGRect(x: 0, y: 0, width: 320, height: max(m.surfaceSize.height, 400))
        view.layoutSubtreeIfNeeded()
        _ = view.fittingSize
        #else
        renderInWindow(root, size: CGSize(width: 320, height: max(m.surfaceSize.height, 400)))
        #endif
    }

    private static let png: Data = SafeHostTests.realPng(4, 3)
    private static var dataSrc: String { "data:image/png;base64,\(png.base64EncodedString())" }

    func testTheModelPolicesTheHostsAnswer() throws {
        let m = try model(SourceHost())
        XCTAssertEqual(m.imageSource("/api/attachments/a", alt: ""), .load)
        XCTAssertEqual(m.imageSource("https://example.com/cat.png", alt: ""), .link(href: "https://example.com/cat.png", text: "Image: example.com"))
        XCTAssertEqual(m.imageSource("https://example.com/cat.png", alt: "A cat"), .link(href: "https://example.com/cat.png", text: "A cat"))
        // A link the URL policy denies (http here) is its text, never a link.
        XCTAssertEqual(m.imageSource("http://example.com/cat.png", alt: ""), .text("Image: example.com"))
        XCTAssertEqual(m.imageSource("file:///etc/passwd", alt: "x"), .text("x"))
        // No hook = the media policy alone decides.
        let plain = try model(NoHost())
        XCTAssertEqual(plain.imageSource("https://example.com/cat.png", alt: ""), .load)
        XCTAssertFalse(plain.host.hasImageMenu)
    }

    func testMarkdownBlockImagesFollowTheSourceRule() throws {
        let host = SourceHost()
        let m = try model(host)
        let blocks = MarkdownPainter.painted("![A cat](https://example.com/cat.png)\n\n![](cat.png)\n\n![x](/api/attachments/a)", model: m)
        XCTAssertEqual(blocks.count, 2, "the alt-less refused one draws nothing")
        XCTAssertEqual(blocks[0].kind, .paragraph)
        XCTAssertEqual(blocks[0].inlines.map(\.text), ["A cat"])
        XCTAssertEqual(blocks[0].inlines.first?.link, "https://example.com/cat.png")
        XCTAssertEqual(blocks[1].kind, .image(src: "/api/attachments/a", alt: "x"))
        XCTAssertFalse(host.asked.contains("https://example.com/cat.png"), "another website's picture is never fetched")
    }

    func testAnImageFromAnotherWebsitePaintsALinkAndIsNeverFetched() throws {
        let host = SourceHost()
        let m = try model(host)
        let tree = JSONValue.object(["id": .string("root"), "component": .string("Stack"), "props": .object([:]), "children": .array([
            .object(["id": .string("ext"), "component": .string("Image"), "props": .object(["src": .string("https://example.com/cat.png"), "width": .number(120), "height": .number(80)])]),
            .object(["id": .string("zoom"), "component": .string("ZoomableImage"), "props": .object(["src": .string("https://example.com/big.png"), "alt": .string("Big"), "width": .number(120), "height": .number(80)])]),
        ])])
        try m.setNested(json: tree.json)
        paint(m)
        XCTAssertFalse(host.asked.contains("https://example.com/cat.png"), "\(host.asked)")
        XCTAssertFalse(host.asked.contains("https://example.com/big.png"), "\(host.asked)")
        XCTAssertTrue(host.menus.isEmpty, "a link carries no image menu")
        XCTAssertTrue(m.paintFailures.isEmpty)
    }

    func testARenderedPictureCarriesTheHostsImageMenuWithItsOriginalBytes() async throws {
        let host = SourceHost()
        let m = try model(host)
        let src = Self.dataSrc
        let request = try XCTUnwrap(m.mediaRequest(src))
        _ = await MediaLoader.shared.load(request, options: m.mediaPolicy)
        let tree = JSONValue.object(["id": .string("root"), "component": .string("Stack"), "props": .object([:]), "children": .array([
            .object(["id": .string("pic"), "component": .string("Image"), "props": .object(["src": .string(src), "alt": .string("red"), "width": .number(40), "height": .number(30)])]),
        ])])
        try m.setNested(json: tree.json)
        paint(m)
        let target = try XCTUnwrap(host.menus.first, "the rendered picture asked the host's image menu")
        XCTAssertEqual(target.src, src)
        XCTAssertEqual(target.alt, "red")
        let bytes = try await target.bytes()
        XCTAssertEqual(bytes, Self.png)
    }

    func testTheOriginalDropsOnlyTheThumbnailWidth() {
        XCTAssertEqual(originalImageSrc("/api/attachments/a?w=480"), "/api/attachments/a")
        XCTAssertEqual(originalImageSrc("/api/attachments/a?token=t&w=480#x"), "/api/attachments/a?token=t#x")
        XCTAssertEqual(originalImageSrc("/api/attachments/a?wide=1"), "/api/attachments/a?wide=1")
        XCTAssertEqual(originalImageSrc("data:image/png;base64,AA?w=1"), "data:image/png;base64,AA?w=1")
    }

    func testAClaimedPressStandsTheContextMenuDown() throws {
        let m = try model(SourceHost())
        XCTAssertFalse(m.imagePressClaimed)
        m.claimImagePress()
        XCTAssertTrue(m.imagePressClaimed)
        m.imagePressClaimedAt = Date().addingTimeInterval(-2)
        XCTAssertFalse(m.imagePressClaimed, "only the current press")
    }

    func testTheBridgesForwardTheHooks() throws {
        let plugin = SourceHost()
        let host = ExponentialHost(HostOptions(plugin: plugin))
        let bridge = HostBridge(host: host)
        XCTAssertTrue(bridge.hasImageMenu)
        XCTAssertEqual(bridge.imageSource("https://example.com/a.png", alt: ""), .link(href: "https://example.com/a.png", text: "Image: example.com"))
        let bare = ExponentialHost(HostOptions())
        let none = HostBridge(host: bare)
        XCTAssertFalse(none.hasImageMenu)
        XCTAssertNil(none.imageSource("https://example.com/a.png", alt: ""))
    }
}
