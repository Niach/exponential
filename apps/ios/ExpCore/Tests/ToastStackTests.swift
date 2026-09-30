import Foundation
import XCTest
@testable import ExpCore

// EXP-1031: the shared stacking toast, locked ×4 (web toast.test.tsx, desktop
// crates/ui toast.rs, Android ToastStackTest) against the ONE contract fixture.
final class ToastStackTests: XCTestCase {
    private struct Fixture: Decodable {
        let constants: FixtureConstants
        let geometry: [GeometryCase]
    }

    private struct FixtureConstants: Decodable {
        let width: Double
        let gap: Double
        let peek: Double
        let scaleStep: Double
        let visible: Int
        let durationMs: Int
        let swipeThreshold: Double
        let viewportOffset: Double
        let mobileViewportOffset: Double
        let kinds: [String]
    }

    private struct GeometryCase: Decodable {
        let name: String
        let heights: [Double]
        let expanded: Bool
        let anchoredBottom: Bool
        let height: Double
        let items: [FixtureItem]
    }

    private struct FixtureItem: Decodable {
        let offset: Double
        let scale: Double
        let visible: Bool
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/toast-stack.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testConstantsMirrorTheFixture() throws {
        let c = try fixture().constants
        typealias K = ToastStack.Constants
        XCTAssertEqual(K.width, c.width)
        XCTAssertEqual(K.gap, c.gap)
        XCTAssertEqual(K.peek, c.peek)
        XCTAssertEqual(K.scaleStep, c.scaleStep)
        XCTAssertEqual(K.visible, c.visible)
        XCTAssertEqual(K.durationMs, c.durationMs)
        XCTAssertEqual(K.swipeThreshold, c.swipeThreshold)
        XCTAssertEqual(K.viewportOffset, c.viewportOffset)
        XCTAssertEqual(K.mobileViewportOffset, c.mobileViewportOffset)
        XCTAssertEqual(K.kinds.map(\.rawValue), c.kinds)
    }

    func testEveryGeometryCase() throws {
        let cases = try fixture().geometry
        XCTAssertFalse(cases.isEmpty)
        for fixtureCase in cases {
            let result = ToastStack.geometry(
                heights: fixtureCase.heights,
                expanded: fixtureCase.expanded,
                anchoredBottom: fixtureCase.anchoredBottom
            )
            XCTAssertEqual(result.height, fixtureCase.height, accuracy: 1e-9, fixtureCase.name)
            XCTAssertEqual(result.items.count, fixtureCase.items.count, fixtureCase.name)
            for (index, (got, want)) in zip(result.items, fixtureCase.items).enumerated() {
                let label = "\(fixtureCase.name) #\(index)"
                XCTAssertEqual(got.offset, want.offset, accuracy: 1e-9, label)
                XCTAssertEqual(got.scale, want.scale, accuracy: 1e-9, label)
                XCTAssertEqual(got.visible, want.visible, label)
            }
        }
    }

    func testQueueAppendsNewestLastAndCaps() {
        var queue = ToastQueue()
        for i in 0..<(ToastQueue.cap + 2) {
            queue.append(ToastItem(kind: .info, title: "t\(i)"))
        }
        XCTAssertEqual(queue.items.count, ToastQueue.cap)
        XCTAssertEqual(queue.items.first?.title, "t2")
        XCTAssertEqual(queue.items.last?.title, "t\(ToastQueue.cap + 1)")
    }

    func testDismissingTheLastToastCollapses() {
        var queue = ToastQueue()
        let item = ToastItem(kind: .error, title: "Nope")
        queue.append(item)
        queue.expanded = true
        queue.dismiss(item.id)
        XCTAssertTrue(queue.items.isEmpty)
        XCTAssertFalse(queue.expanded)
    }
}
