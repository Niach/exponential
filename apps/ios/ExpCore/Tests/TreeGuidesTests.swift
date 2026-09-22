import XCTest
@testable import ExpCore

/// EXP-965: the connector's pure rule — the shape a nested list draws, keyed
/// only by its visible rows' depths.
final class TreeGuidesTests: XCTestCase {

    func testARootRowDrawsNothing() {
        let guides = TreeGuides.compute(depths: [0, 0, 0])
        XCTAssertEqual(guides, [TreeGuide(), TreeGuide(), TreeGuide()])
        XCTAssertTrue(guides.allSatisfy(\.isEmpty))
    }

    func testAChainElbowsOncePerLevelAndNeverTees() {
        // parent → child → grandchild: each is its parent's LAST (only) child,
        // so every guide is a bare elbow with nothing passing through.
        let guides = TreeGuides.compute(depths: [0, 1, 2, 3])
        XCTAssertEqual(guides[0], TreeGuide())
        XCTAssertEqual(guides[1], TreeGuide(elbowAt: 0, tee: false, passThrough: []))
        XCTAssertEqual(guides[2], TreeGuide(elbowAt: 1, tee: false, passThrough: []))
        XCTAssertEqual(guides[3], TreeGuide(elbowAt: 2, tee: false, passThrough: []))
        XCTAssertEqual(guides.map(\.depth), [0, 1, 2, 3])
    }

    func testSiblingsTeeUntilTheLastOne() {
        let guides = TreeGuides.compute(depths: [0, 1, 1, 1])
        XCTAssertTrue(guides[1].tee)
        XCTAssertTrue(guides[2].tee)
        XCTAssertFalse(guides[3].tee)
        XCTAssertEqual(guides[3], TreeGuide(elbowAt: 0, tee: false, passThrough: []))
    }

    func testNestedSiblingsPassTheirAncestorsLineThrough() {
        //  0  root
        //  1  ├ a
        //  2  │ ├ a1        (root's subtree continues → level 0 passes through)
        //  2  │ └ a2
        //  1  └ b
        let guides = TreeGuides.compute(depths: [0, 1, 2, 2, 1])
        XCTAssertEqual(guides[1], TreeGuide(elbowAt: 0, tee: true, passThrough: []))
        XCTAssertEqual(guides[2], TreeGuide(elbowAt: 1, tee: true, passThrough: [0]))
        XCTAssertEqual(guides[3], TreeGuide(elbowAt: 1, tee: false, passThrough: [0]))
        XCTAssertEqual(guides[4], TreeGuide(elbowAt: 0, tee: false, passThrough: []))
    }

    func testTheLastChildOfALastChildDropsEveryLine() {
        //  0 root · 1 a · 2 a1 · 1 b · 2 b1 · 3 b1a
        let guides = TreeGuides.compute(depths: [0, 1, 2, 1, 2, 3])
        // a1 hangs under a, which still has b below it at level 0.
        XCTAssertEqual(guides[2], TreeGuide(elbowAt: 1, tee: false, passThrough: [0]))
        // b is the last root child; nothing below it continues any level.
        XCTAssertEqual(guides[4], TreeGuide(elbowAt: 1, tee: false, passThrough: []))
        XCTAssertEqual(guides[5], TreeGuide(elbowAt: 2, tee: false, passThrough: []))
    }

    func testTheIndentIsTheSharedFourteenPointMeasure() {
        XCTAssertEqual(TreeGuides.indentPerLevel, 14)
        XCTAssertEqual(TreeGuides.elbowRadius, 3)
    }
}
