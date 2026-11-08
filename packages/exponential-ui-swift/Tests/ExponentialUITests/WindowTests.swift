import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// VAPP-290: the `Window` macro over Box / Text: a 34-tall chrome bar (three
/// 10 × 10 dots at the inline start, the title centred, hidden from AT) above
/// a body that holds the one child and fills the rest; the root is a group
/// named by its title.
@MainActor
final class WindowTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    private func model(_ tree: String) throws -> SurfaceModel {
        var options = SurfaceOptions()
        options.theme = ThemeHandle.builtin("neutral")
        options.mode = .dark
        let m = try SurfaceModel(id: "win", options: options, host: RecordingHost())
        try m.setNested(json: tree)
        m.setViewport(width: 390, height: 900)
        return m
    }

    private func node(_ m: SurfaceModel, _ id: String) throws -> NodeInfo {
        try XCTUnwrap(m.node(id: id), id)
    }

    private let titled = #"{"id":"w","component":"Window","props":{"title":"Preview"},"style":{"height":200},"children":[{"id":"content","component":"Text","props":{"text":"Hello"}}]}"#
    private let untitled = #"{"id":"w","component":"Window","style":{"height":200},"children":[{"id":"content","component":"Text","props":{"text":"Hello"}}]}"#

    /// The chrome: 34 tall at the root's top, three 10 × 10 dots, the first
    /// inside the 1 px border + the `md` padding; the title centred within
    /// the root (the `end` spacer balances the dots).
    func testChromeDotsAndCentredTitle() throws {
        let m = try model(titled)
        XCTAssertNil(m.nodes.first { $0.component == "Unknown" })
        let root = m.frame(try node(m, "w").index)
        XCTAssertEqual(root.height, 200, accuracy: 0.5)
        XCTAssertGreaterThan(root.width, 0)

        let chromeNode = try node(m, "w.chrome")
        let chrome = m.frame(chromeNode.index)
        XCTAssertEqual(chrome.height, 34, accuracy: 0.5)
        XCTAssertEqual(chrome.minY, root.minY + 1, accuracy: 0.5, "at the top, inside the border")

        let dots = try (0..<3).map { try node(m, "w.chrome.dots.dot.\($0)") }
        for d in dots {
            XCTAssertEqual(m.frame(d.index).size.width, 10, accuracy: 0.5)
            XCTAssertEqual(m.frame(d.index).size.height, 10, accuracy: 0.5)
        }
        XCTAssertEqual(m.frame(dots[0].index).minX, root.minX + 1 + 12, accuracy: 0.5)
        XCTAssertEqual(m.frame(dots[1].index).minX - m.frame(dots[0].index).maxX, 6, accuracy: 0.5)
        XCTAssertEqual(m.frame(dots[0].index).midY, chrome.midY, accuracy: 0.5)

        let title = try node(m, "w.chrome.title")
        XCTAssertEqual(title.component, "Text")
        XCTAssertEqual(title.props["text"]?.string, "Preview")
        XCTAssertEqual(m.frame(title.index).midX, root.midX, accuracy: 0.5)
    }

    /// The body sits under the chrome and fills the rest; the child is inside it.
    func testBodyFillsTheRest() throws {
        let m = try model(titled)
        let root = m.frame(try node(m, "w").index)
        let chrome = m.frame(try node(m, "w.chrome").index)
        let body = m.frame(try node(m, "w.body").index)
        XCTAssertEqual(body.minY, chrome.maxY, accuracy: 0.5)
        XCTAssertEqual(body.maxY, root.maxY - 1, accuracy: 0.5, "to the bottom, inside the border")
        XCTAssertEqual(body.height, root.height - 34 - 2, accuracy: 0.5)
        let child = m.frame(try node(m, "content").index)
        XCTAssertTrue(body.insetBy(dx: -0.5, dy: -0.5).contains(child), "\(child) inside \(body)")
    }

    /// The root is a group named by the title; the chrome is hidden from AT.
    func testAccessibility() throws {
        let m = try model(titled)
        let root = try node(m, "w")
        let a = A11yInfo.of(root, model: m, style: m.style(root.index))
        XCTAssertEqual(a.role, "group")
        XCTAssertEqual(a.label, "Preview")
        let chrome = try node(m, "w.chrome")
        XCTAssertTrue(A11yInfo.of(chrome, model: m, style: m.style(chrome.index)).hidden)
    }

    /// No title = no title node; the dots stay.
    func testUntitledHasNoTitle() throws {
        let m = try model(untitled)
        XCTAssertNil(m.nodes.first { $0.component == "Unknown" })
        XCTAssertNil(m.node(id: "w.chrome.title"))
        XCTAssertNotNil(m.node(id: "w.chrome.end"))
        XCTAssertNotNil(m.node(id: "w.chrome.dots.dot.2"))
    }
}
