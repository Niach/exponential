import Foundation
import XCTest
@testable import ExpCore

// EXP-897 — the PR stack's three rules, the same three web
// (`pr-stack.test.ts`), Android (`PrStackTest`) and the desktop
// (`nest_review_entries_*`) run.
final class PrStackTests: XCTestCase {
    private struct Entry {
        let id: String
        let branch: String?
        let base: String?
    }

    private func chain(_ entry: Entry, _ entries: [Entry]) -> [String] {
        PrStack.stackChain(
            entry, in: entries, id: { $0.id }, branch: { $0.branch }, base: { $0.base }
        ).map(\.id)
    }

    private func position(_ entry: Entry, _ entries: [Entry]) -> String? {
        guard let spot = PrStack.stackPosition(
            entry, in: entries, id: { $0.id }, branch: { $0.branch }, base: { $0.base }
        ) else { return nil }
        return "\(spot.position)/\(spot.size) below=\(spot.below?.id ?? "-") above=\(spot.above?.id ?? "-")"
    }

    private func nested(_ entries: [Entry]) -> [String] {
        PrStack.nestPrStacks(
            entries, id: { $0.id }, branch: { $0.branch }, base: { $0.base }
        ).map { "\($0.entry.id)@\($0.depth)\($0.hasChildren ? "+" : "")" }
    }

    // A member knows its rung from the BOTTOM of the chain, whatever order the
    // caller hands the rows in.
    func testNumbersAMemberFromTheBottomOfTheChain() {
        let a = Entry(id: "a", branch: "exp/a", base: "main")
        let b = Entry(id: "b", branch: "exp/b", base: "exp/a")
        let c = Entry(id: "c", branch: "exp/c", base: "exp/b")
        let entries = [c, a, b]
        XCTAssertEqual(chain(b, entries), ["a", "b", "c"])
        XCTAssertEqual(position(a, entries), "1/3 below=- above=b")
        XCTAssertEqual(position(b, entries), "2/3 below=a above=c")
        XCTAssertEqual(position(c, entries), "3/3 below=b above=-")
        // Roots keep the caller's order; children follow their parent.
        XCTAssertEqual(nested(entries), ["a@0+", "b@1+", "c@2"])
    }

    // `main` is nobody's branch here, so the walk stops: a PR based on it is a
    // root, and a lone PR is no stack at all.
    func testStopsAtABaseNobodyInTheListOwns() {
        let a = Entry(id: "a", branch: "exp/a", base: "main")
        let lone = Entry(id: "lone", branch: "exp/lone", base: nil)
        let orphan = Entry(id: "orphan", branch: "exp/orphan", base: "exp/gone")
        // An EMPTY base is not an edge either.
        let blank = Entry(id: "blank", branch: "exp/blank", base: "")
        let entries = [a, lone, orphan, blank]
        XCTAssertEqual(chain(a, entries), ["a"])
        XCTAssertNil(position(a, entries))
        XCTAssertNil(position(orphan, entries))
        XCTAssertEqual(nested(entries), ["a@0", "lone@0", "orphan@0", "blank@0"])
    }

    // A cycle (defensive — GitHub cannot make one, a half-synced snapshot can)
    // breaks where it first repeats, and never loses a row.
    func testBreaksACycleWhereItFirstAppears() {
        let a = Entry(id: "a", branch: "exp/a", base: "exp/b")
        let b = Entry(id: "b", branch: "exp/b", base: "exp/a")
        let selfish = Entry(id: "self", branch: "exp/self", base: "exp/self")
        let entries = [a, b, selfish]
        XCTAssertEqual(chain(a, entries), ["b", "a"])
        XCTAssertEqual(nested(entries), ["self@0", "a@0+", "b@1"])
    }
}
