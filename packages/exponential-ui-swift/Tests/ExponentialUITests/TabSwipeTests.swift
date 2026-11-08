import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// FEED-200: `Tabs.swipeable`. `fixtures/swipe.json` replayed through the
/// FFI (the groups it exposes: lock, offset, settle, blocks); the model's
/// swipe events on a swipeable Tabs (the neighbours one width aside, the
/// pages following the finger, the settle, the spring back, the cancel,
/// RTL) and the hit rule that leaves a sideways-panning node its touch.
@MainActor
final class TabSwipeTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    // MARK: - the fixture through the FFI

    private func cases(_ f: JSONValue, _ group: String) -> [JSONValue] { f[group]?.array ?? [] }

    private func num(_ c: JSONValue, _ key: String) -> Double { c[key]?.number ?? .nan }

    private func name(_ c: JSONValue) -> String { c["name"]?.string ?? c.json }

    func testTheLockGroupReplays() throws {
        let f = try Fixtures.json("swipe.json")
        let all = cases(f, "lock")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            XCTAssertEqual(swipeLock(dx: num(c, "dx"), dy: num(c, "dy")), c["lock"]?.string, name(c))
        }
    }

    func testTheOffsetGroupReplays() throws {
        let f = try Fixtures.json("swipe.json")
        let all = cases(f, "offset")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let got = swipeOffset(index: UInt32(num(c, "index")), count: UInt32(num(c, "count")), offset: num(c, "offset"), width: num(c, "width"), rtl: c["rtl"]?.bool ?? false)
            XCTAssertEqual(got, num(c, "drawn"), accuracy: 1e-6, name(c))
        }
    }

    func testTheSettleGroupReplays() throws {
        let f = try Fixtures.json("swipe.json")
        let all = cases(f, "settle")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let got = swipeSettle(index: UInt32(num(c, "index")), count: UInt32(num(c, "count")), offset: num(c, "offset"), velocity: num(c, "velocity"), width: num(c, "width"), rtl: c["rtl"]?.bool ?? false)
            XCTAssertEqual(Double(got), num(c, "settle"), name(c))
        }
    }

    func testTheBlocksGroupReplays() throws {
        let f = try Fixtures.json("swipe.json")
        let all = cases(f, "blocks")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let got = swipeBlocks(component: c["component"]?.string ?? "", direction: c["direction"]?.string, overflowX: c["overflowX"]?.string)
            XCTAssertEqual(got, c["blocks"]?.bool, name(c))
        }
    }

    // MARK: - the surface

    private func tree(direction: String = "ltr") -> String {
        """
        {"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "direction": "\(direction)"}, "children": [
          {"id": "swipe_tabs", "component": "Tabs", "props": {"swipeable": true, "fill": true, "value": "t1", "tabs": [{"label": "Dashboard", "value": "t0"}, {"label": "Activity", "value": "t1"}, {"label": "Income", "value": "t2"}, {"label": "Expenses", "value": "t3"}]}, "children": [
            {"id": "panel_0", "component": "Text", "props": {"text": "Panel 0"}},
            {"id": "panel_1", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
              {"id": "panel_text", "component": "Text", "props": {"text": "Panel 1"}},
              {"id": "panel_slider", "component": "Slider", "props": {"value": 50, "min": 0, "max": 100}}
            ]},
            {"id": "panel_2", "component": "Text", "props": {"text": "Panel 2"}},
            {"id": "panel_3", "component": "Text", "props": {"text": "Panel 3"}}
          ]}
        ]}
        """
    }

    private func model(direction: String = "ltr") throws -> SurfaceModel {
        var options = SurfaceOptions()
        options.theme = ThemeHandle.builtin("exponential")
        let m = try SurfaceModel(id: "swipe", options: options, host: NoHost())
        _ = try m.setNested(json: tree(direction: direction))
        m.setViewport(width: 400, height: 800)
        return m
    }

    private func dx(_ m: SurfaceModel, _ id: String) -> CGFloat {
        m.index(of: id).map { m.style($0).transform.tx } ?? .nan
    }

    private func shown(_ m: SurfaceModel, _ id: String) -> Bool {
        m.node(id: id).map { !$0.hidden } ?? false
    }

    private func selected(_ m: SurfaceModel, _ id: String) -> Bool {
        m.node(id: id)?.selected ?? false
    }

    private func content(_ m: SurfaceModel) throws -> (Int, CGFloat) {
        let i = try XCTUnwrap(m.index(of: "swipe_tabs.content"))
        return (i, m.frame(i).width)
    }

    func testTheNeighboursWaitOneWidthAsideAndTheThumbSitsOnTheActiveTab() throws {
        let m = try model()
        let (c, width) = try content(m)
        XCTAssertTrue(m.isSwipeContent(c))
        XCTAssertFalse(m.isSwipeContent(try XCTUnwrap(m.index(of: "swipe_tabs.list"))))
        XCTAssertGreaterThan(width, 300)
        XCTAssertTrue(m.style(c).clipX && m.style(c).clipY, "the content clips the neighbours")
        XCTAssertTrue(shown(m, "panel_0") && shown(m, "panel_1") && shown(m, "panel_2"))
        XCTAssertFalse(shown(m, "panel_3"), "only the neighbours join")
        XCTAssertEqual(dx(m, "panel_0"), -width, accuracy: 0.5)
        XCTAssertEqual(dx(m, "panel_1"), 0, accuracy: 0.5)
        XCTAssertEqual(dx(m, "panel_2"), width, accuracy: 0.5)
        let n0 = try XCTUnwrap(m.index(of: "panel_0"))
        XCTAssertTrue(m.style(n0).pointerNone, "a neighbour is inert")
        let thumb = try XCTUnwrap(m.index(of: "swipe_tabs.thumb"))
        let tab1 = try XCTUnwrap(m.index(of: "swipe_tabs.tab.1"))
        XCTAssertTrue(m.style(thumb).pointerNone)
        let list = try XCTUnwrap(m.index(of: "swipe_tabs.list"))
        XCTAssertEqual(m.style(thumb).transform.tx, m.frame(tab1).minX - m.frame(list).minX, accuracy: 0.5)
        XCTAssertEqual(m.frame(thumb).width, m.frame(tab1).width, accuracy: 0.5)
    }

    func testASwipeFollowsTheFingerThenAFlingSettlesOnTheNextTab() throws {
        let m = try model()
        let (c, width) = try content(m)
        m.tabSwipe(c, phase: "start", offset: 0)
        m.tabSwipe(c, phase: "move", offset: -width / 4)
        XCTAssertEqual(dx(m, "panel_1"), -width / 4, accuracy: 0.5)
        XCTAssertEqual(dx(m, "panel_2"), width * 0.75, accuracy: 0.5)
        XCTAssertNil(m.style(try XCTUnwrap(m.index(of: "panel_1"))).transition?.animation(), "no transition under the finger")
        m.tabSwipe(c, phase: "end", offset: -width / 4, velocity: -0.6)
        XCTAssertTrue(selected(m, "swipe_tabs.tab.2"), "a fling pages")
        XCTAssertFalse(selected(m, "swipe_tabs.tab.1"))
        XCTAssertEqual(dx(m, "panel_2"), 0, accuracy: 0.5)
        XCTAssertEqual(dx(m, "panel_1"), -width, accuracy: 0.5)
        XCTAssertEqual(dx(m, "panel_3"), width, accuracy: 0.5)
        XCTAssertFalse(shown(m, "panel_0"))
        let p2 = try XCTUnwrap(m.index(of: "panel_2"))
        XCTAssertNotNil(m.style(p2).transition?.animation(reduceMotion: false), "the settle animates")
        XCTAssertNil(m.style(p2).transition?.animation(reduceMotion: true), "reduced motion snaps")
    }

    func testAShortSlowSwipeSpringsBackAndACancelRests() throws {
        let m = try model()
        let (c, width) = try content(m)
        m.tabSwipe(c, phase: "start", offset: 0)
        m.tabSwipe(c, phase: "move", offset: -width / 4)
        m.tabSwipe(c, phase: "end", offset: -width / 4, velocity: -0.1)
        XCTAssertTrue(selected(m, "swipe_tabs.tab.1"), "a slow short drag stays")
        XCTAssertEqual(dx(m, "panel_1"), 0, accuracy: 0.5)
        m.tabSwipe(c, phase: "start", offset: 0)
        m.tabSwipe(c, phase: "move", offset: width * 0.8)
        XCTAssertEqual(dx(m, "panel_1"), width * 0.8, accuracy: 0.5)
        m.tabSwipe(c, phase: "cancel", offset: 0)
        XCTAssertTrue(selected(m, "swipe_tabs.tab.1"), "a cancel never pages")
        XCTAssertEqual(dx(m, "panel_1"), 0, accuracy: 0.5)
        // A slow drag past half pages back.
        m.tabSwipe(c, phase: "start", offset: 0)
        m.tabSwipe(c, phase: "end", offset: width * 0.6, velocity: 0)
        XCTAssertTrue(selected(m, "swipe_tabs.tab.0"))
        // Page 0 dragged right resists (no neighbour there).
        m.tabSwipe(c, phase: "start", offset: 0)
        m.tabSwipe(c, phase: "move", offset: 100)
        XCTAssertEqual(dx(m, "panel_0"), 30, accuracy: 0.5)
        m.tabSwipe(c, phase: "end", offset: 100, velocity: 1)
        XCTAssertTrue(selected(m, "swipe_tabs.tab.0"), "never before the first page")
    }

    func testRtlRevealsTheNextTabToTheLeft() throws {
        let m = try model(direction: "rtl")
        let (c, width) = try content(m)
        XCTAssertEqual(dx(m, "panel_0"), width, accuracy: 0.5)
        XCTAssertEqual(dx(m, "panel_2"), -width, accuracy: 0.5)
        // The SCREEN offset, never mirrored by the painter: right = next.
        m.tabSwipe(c, phase: "start", offset: 0)
        m.tabSwipe(c, phase: "end", offset: width * 0.6, velocity: 0)
        XCTAssertTrue(selected(m, "swipe_tabs.tab.2"))
    }

    func testANodeThatPansSidewaysKeepsItsTouch() throws {
        let m = try model()
        let (c, _) = try content(m)
        let slider = m.frame(try XCTUnwrap(m.index(of: "panel_slider")))
        let text = m.frame(try XCTUnwrap(m.index(of: "panel_text")))
        XCTAssertTrue(m.swipeBlocked(c, at: CGPoint(x: slider.midX, y: slider.midY)), "a Slider drags itself")
        XCTAssertFalse(m.swipeBlocked(c, at: CGPoint(x: text.midX, y: text.midY)), "text is the pager's")
        // The neighbours (stacked over the active page, later in paint
        // order) are inert: the hit is the active page's text.
        XCTAssertFalse(m.swipeBlocked(c, at: CGPoint(x: m.frame(c).maxX - 1, y: text.midY)))
        XCTAssertTrue(m.swipeBlocked(c, at: CGPoint(x: slider.maxX - 1, y: slider.midY)), "past the neighbours to the Slider")
    }

    func testTheAxisLockIsTheCore() throws {
        let m = try model()
        XCTAssertEqual(m.swipeAxis(dx: 5, dy: 3), "pending")
        XCTAssertEqual(m.swipeAxis(dx: -9, dy: 2), "horizontal")
        XCTAssertEqual(m.swipeAxis(dx: 10, dy: -10), "vertical")
    }

    /// A Tabs without `swipeable` ignores the event and draws no pager.
    func testANonSwipeableTabsNeverPages() throws {
        var options = SurfaceOptions()
        options.theme = ThemeHandle.builtin("exponential")
        let m = try SurfaceModel(id: "plain", options: options, host: NoHost())
        _ = try m.setNested(json: #"{"id": "plain_tabs", "component": "Tabs", "props": {"value": "a", "tabs": [{"label": "A", "value": "a"}, {"label": "B", "value": "b"}]}, "children": [{"id": "panel_a", "component": "Text", "props": {"text": "A"}}, {"id": "panel_b", "component": "Text", "props": {"text": "B"}}]}"#)
        m.setViewport(width: 400, height: 800)
        let c = try XCTUnwrap(m.index(of: "plain_tabs.content"))
        XCTAssertFalse(m.isSwipeContent(c))
        XCTAssertNil(m.index(of: "plain_tabs.thumb"))
        m.tabSwipe(c, phase: "end", offset: -300, velocity: -1)
        XCTAssertTrue(m.node(id: "plain_tabs.tab.0")?.selected ?? false)
    }
}
