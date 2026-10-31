import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// EXP-1495 (docs/round-6-contract.md, the SwiftUI checklist): the Carousel
/// `counter` pill + `thumbnails` strip from the core's frames and recipes, a
/// thumbnail press paging like a dot, and the Outline + Mosaic macros over
/// Box / Text / Image.
@MainActor
final class Round6Tests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    private func model(_ tree: String, host: HostPlugin) throws -> SurfaceModel {
        var options = SurfaceOptions()
        options.theme = ThemeHandle.builtin("neutral")
        options.mode = .dark
        let m = try SurfaceModel(id: "r6", options: options, host: host)
        try m.setNested(json: tree)
        m.setViewport(width: 390, height: 900)
        return m
    }

    private func parts(_ m: SurfaceModel, _ part: String) -> [NodeInfo] {
        m.nodes.filter { $0.part == part && !$0.removed }
    }

    private let carousel = #"{"id":"c","component":"Carousel","props":{"counter":true,"thumbnails":["https://example.com/1.jpg","","https://example.com/3.jpg","https://example.com/extra.jpg"]},"on":{"change":{"event":{"name":"paged"}}},"children":[{"id":"p1","component":"Text","props":{"text":"One"}},{"id":"p2","component":"Text","props":{"text":"Two"}},{"id":"p3","component":"Text","props":{"text":"Three"}}]}"#

    // MARK: - Carousel counter

    /// `$string.pageCounter` over the track's top-start corner (inset sm),
    /// the neutral pill (20 tall, full radius, black 0.55), hidden from AT.
    func testCounterPillSitsOverTheTrackCorner() throws {
        let m = try model(carousel, host: RecordingHost())
        let counter = try XCTUnwrap(parts(m, "counter").first)
        XCTAssertEqual(counter.component, "Text")
        XCTAssertEqual(counter.props["text"]?.string, "1 / 3")
        let page = try XCTUnwrap(parts(m, "page").first)
        let (cf, pf) = (m.frame(counter.index), m.frame(page.index))
        XCTAssertEqual(cf.minX - pf.minX, 8, accuracy: 0.5)
        XCTAssertEqual(cf.minY - pf.minY, 8, accuracy: 0.5)
        XCTAssertEqual(cf.height, 20, accuracy: 0.5)
        let style = m.style(counter.index)
        XCTAssertNotNil(style.background)
        XCTAssertTrue(A11yInfo.of(counter, model: m, style: style).hidden)
    }

    // MARK: - Carousel thumbnails

    /// One 52 × 52 thumbnail per PAGE between the track and the controls
    /// (the extra URL ignored, the empty one = no picture), the current one
    /// `selected`; a press pages like a dot and fires `change {page}`.
    func testThumbnailStripPagesOnPress() throws {
        let host = RecordingHost()
        let m = try model(carousel, host: host)
        let thumbs = parts(m, "thumbnail")
        XCTAssertEqual(thumbs.count, 3)
        for t in thumbs {
            XCTAssertEqual(m.frame(t.index).size, CGSize(width: 52, height: 52))
            XCTAssertTrue(t.pressable)
        }
        XCTAssertEqual(parts(m, "thumbnailImage").count, 2, "the empty URL paints the muted square")
        let strip = try XCTUnwrap(parts(m, "thumbnails").first)
        let page = try XCTUnwrap(parts(m, "page").first)
        let controls = try XCTUnwrap(parts(m, "controls").first)
        XCTAssertGreaterThanOrEqual(m.frame(strip.index).minY, m.frame(page.index).maxY)
        XCTAssertLessThanOrEqual(m.frame(strip.index).maxY, m.frame(controls.index).minY)
        XCTAssertTrue(thumbs[0].states.contains("selected"))
        XCTAssertFalse(thumbs[2].states.contains("selected"))
        XCTAssertEqual(A11yInfo.of(thumbs[1], model: m, style: m.style(thumbs[1].index)).label, "Page 2 of 3")

        m.press(thumbs[2].index)
        XCTAssertEqual(host.actions.map(\.name), ["paged"])
        XCTAssertEqual(host.actions.first?.event, "change")
        XCTAssertEqual(host.actions.first?.payload?["page"]?.number, 2)
        XCTAssertEqual(parts(m, "counter").first?.props["text"]?.string, "3 / 3")
        XCTAssertTrue(parts(m, "thumbnail")[2].states.contains("selected"))
    }

    /// A single page: no counter, no strip (like the controls row).
    func testOnePageHasNoExtras() throws {
        let m = try model(#"{"id":"c","component":"Carousel","props":{"counter":true,"thumbnails":["https://example.com/1.jpg"]},"children":[{"id":"p1","component":"Text","props":{"text":"One"}}]}"#, host: RecordingHost())
        XCTAssertTrue(parts(m, "counter").isEmpty)
        XCTAssertTrue(parts(m, "thumbnails").isEmpty)
    }

    // MARK: - Outline + Mosaic

    private func node(_ m: SurfaceModel, _ id: String) throws -> NodeInfo {
        try XCTUnwrap(m.node(id: id), id)
    }

    /// The Outline: one 32-tall row per item, the active one with its 2 px
    /// bar at the inline start, the `· n` count only when n > 0; a row
    /// press = `select {id}`.
    func testOutlineRowsAndSelect() throws {
        let host = RecordingHost()
        let m = try model(#"{"id":"o","component":"Outline","props":{"caption":"Outline","value":"part-a","items":[{"id":"current","title":"Current state"},{"id":"part-a","title":"Part A","count":5}]},"on":{"select":{"event":{"name":"jump"}}}}"#, host: host)
        XCTAssertNil(m.nodes.first { $0.component == "Unknown" })
        let rows = [try node(m, "o.row.0"), try node(m, "o.row.1")]
        for r in rows {
            XCTAssertEqual(m.frame(r.index).height, 32, accuracy: 0.5)
            XCTAssertTrue(r.pressable)
        }
        XCTAssertNil(m.node(id: "o.row.0.bar"))
        let bar = m.frame(try node(m, "o.row.1.bar").index)
        XCTAssertEqual(bar.width, 2, accuracy: 0.5)
        XCTAssertEqual(bar.minX, m.frame(rows[1].index).minX, accuracy: 0.5)
        XCTAssertEqual(bar.height, 32, accuracy: 0.5)
        XCTAssertNil(m.node(id: "o.row.0.count"))
        XCTAssertEqual(try node(m, "o.row.1.count").props["text"]?.string, "· 5")
        m.press(rows[0].index)
        XCTAssertEqual(host.actions.map(\.name), ["jump"])
        XCTAssertEqual(host.actions.first?.context["id"]?.string, "current", "the routed select carries its id")
    }

    /// A bound `value`: a row press writes the id back, the bar follows.
    func testOutlineBoundValueMovesTheBar() throws {
        let m = try model(#"{"id":"o","component":"Outline","props":{"value":{"path":"/value"},"items":[{"id":"current","title":"Current state"},{"id":"part-a","title":"Part A"}]}}"#, host: RecordingHost())
        m.setData(path: "/value", value: .string("part-a"))
        XCTAssertNotNil(m.node(id: "o.row.1.bar"))
        m.press(try node(m, "o.row.0").index)
        XCTAssertNotNil(m.node(id: "o.row.0.bar"))
        XCTAssertNil(m.node(id: "o.row.1.bar"))
    }

    /// The Mosaic: 1.6 aspect, the 60 % lead beside the side column, the
    /// `+1` over the third tile, every picture `cover` inside its tile's
    /// hairline border; a press = `select {index}`.
    func testMosaicTilesAndSelect() throws {
        let host = RecordingHost()
        let m = try model(#"{"id":"mo","component":"Mosaic","props":{"label":"Screens","pictures":[{"src":"https://example.com/1.jpg","alt":"One"},{"src":"https://example.com/2.jpg","alt":"Two"},{"src":"https://example.com/3.jpg","alt":"Three"},{"src":"https://example.com/4.jpg","alt":"Four"}]},"on":{"select":{"event":{"name":"open"}}}}"#, host: host)
        XCTAssertNil(m.nodes.first { $0.component == "Unknown" })
        let rf = m.frame(try node(m, "mo").index)
        XCTAssertEqual(rf.width / rf.height, 1.6, accuracy: 0.02)
        let lead = m.frame(try node(m, "mo.lead").index)
        XCTAssertEqual(lead.height, rf.height, accuracy: 0.5)
        XCTAssertEqual(lead.width, rf.width * 0.6, accuracy: 0.5)
        XCTAssertEqual(try node(m, "mo.side.bottom.more.moreLabel").props["text"]?.string, "+1")
        for tile in ["mo.lead", "mo.side.top", "mo.side.bottom"] {
            let t = try node(m, tile)
            XCTAssertTrue(t.pressable, tile)
            let img = try node(m, "\(tile).image")
            XCTAssertEqual(img.props["fit"]?.string, "cover")
            XCTAssertEqual(m.frame(img.index), m.frame(t.index).insetBy(dx: 1, dy: 1), tile)
        }
        m.press(try node(m, "mo.side.bottom").index)
        XCTAssertEqual(host.actions.map(\.name), ["open"])
        XCTAssertEqual(host.actions.first?.context["index"]?.number, 2, "the routed select carries its index")
    }
}
