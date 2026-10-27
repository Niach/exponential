import Foundation
import XCTest
@testable import ExpCore

// EXP-1275 (EXP-1259 D8): the feedback-states constants and the mark's
// timeline, locked ×4 against `feedback-states.json` — the same `mark.cases`
// web (`feedback-mark.test.ts`), the desktop (`ui::feedback::mark`) and
// Android (`FeedbackMarkTest`) replay.
final class FeedbackMarkTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/feedback-states.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func block(_ key: String) throws -> [String: Any] {
        try XCTUnwrap(try fixture()[key] as? [String: Any], key)
    }

    func testTheMarkTimelineReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try block("mark")["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let phase = try XCTUnwrap((c["phase"] as? NSNumber)?.doubleValue)
            let spans = try XCTUnwrap(c["spans"] as? [Any])
            XCTAssertEqual(spans.count, FeedbackStates.Mark.stripes.count)
            for (index, raw) in spans.enumerated() {
                let got = FeedbackStates.Mark.stripeSpan(index, phase: phase)
                if let pair = raw as? [NSNumber] {
                    let want = (pair[0].doubleValue, pair[1].doubleValue)
                    let span = try XCTUnwrap(got, "phase \(phase) stripe \(index)")
                    XCTAssertEqual(span.lowerBound, want.0, accuracy: 1e-9, "phase \(phase) stripe \(index)")
                    XCTAssertEqual(span.upperBound, want.1, accuracy: 1e-9, "phase \(phase) stripe \(index)")
                } else {
                    XCTAssertNil(got, "phase \(phase) stripe \(index)")
                }
            }
        }
        XCTAssertNil(FeedbackStates.Mark.stripeSpan(3, phase: 0.5))
    }

    func testTheStripesAndTimingAreTheFixtures() throws {
        let mark = try block("mark")
        XCTAssertEqual(FeedbackStates.Mark.loopMs, mark["loopMs"] as? Int)
        XCTAssertEqual(FeedbackStates.Mark.samples, mark["samples"] as? Int)
        XCTAssertEqual(FeedbackStates.Mark.stripeWidth, (mark["stripeWidth"] as? NSNumber)?.doubleValue)
        let sizes = try XCTUnwrap(mark["sizes"] as? [String: NSNumber])
        XCTAssertEqual(FeedbackStates.Mark.inlineSize, sizes["inline"]?.doubleValue)
        XCTAssertEqual(FeedbackStates.Mark.statusSize, sizes["status"]?.doubleValue)
        XCTAssertEqual(FeedbackStates.Mark.styleguideSize, sizes["styleguide"]?.doubleValue)
        let stripes = try XCTUnwrap(mark["stripes"] as? [[String: Any]])
        XCTAssertEqual(stripes.count, FeedbackStates.Mark.stripes.count)
        for (mine, want) in zip(FeedbackStates.Mark.stripes, stripes) {
            let cubic = try XCTUnwrap(want["cubic"] as? [[NSNumber]])
            XCTAssertEqual(mine.cubic.map(\.x), cubic.map { $0[0].doubleValue })
            XCTAssertEqual(mine.cubic.map(\.y), cubic.map { $0[1].doubleValue })
            let draw = try XCTUnwrap(want["draw"] as? [NSNumber])
            let erase = try XCTUnwrap(want["erase"] as? [NSNumber])
            XCTAssertEqual(mine.draw.from, draw[0].doubleValue)
            XCTAssertEqual(mine.draw.to, draw[1].doubleValue)
            XCTAssertEqual(mine.erase.from, erase[0].doubleValue)
            XCTAssertEqual(mine.erase.to, erase[1].doubleValue)
        }
    }

    /// The loop's phase 0 is the plain disc; the mark never shows it when it
    /// does not move: a non-animating mark is the static logo, and the loop
    /// is entered at its rest, where every cut is whole.
    func testTheMarkRestsOnTheStaticLogo() {
        XCTAssertEqual(FeedbackStates.Mark.spans(phase: 0).compactMap { $0 }.filter { $0.upperBound > $0.lowerBound }, [])
        XCTAssertEqual(FeedbackStates.Mark.staticSpans, [0...1, 0...1, 0...1])
        XCTAssertEqual(FeedbackStates.Mark.restPhase, 0.545, accuracy: 1e-9)
        XCTAssertEqual(FeedbackStates.Mark.spans(phase: FeedbackStates.Mark.restPhase), FeedbackStates.Mark.staticSpans)
        XCTAssertEqual(FeedbackStates.Mark.phase(sinceStart: 0), FeedbackStates.Mark.restPhase, accuracy: 1e-9)
        XCTAssertEqual(
            FeedbackStates.Mark.spans(phase: FeedbackStates.Mark.phase(sinceStart: 0)),
            FeedbackStates.Mark.staticSpans
        )
        // A full loop later it is back at rest; half a loop later it moves.
        XCTAssertEqual(FeedbackStates.Mark.phase(sinceStart: 1.2), FeedbackStates.Mark.restPhase, accuracy: 1e-9)
        XCTAssertNotEqual(
            FeedbackStates.Mark.spans(phase: FeedbackStates.Mark.phase(sinceStart: 0.6)),
            FeedbackStates.Mark.staticSpans
        )
    }

    func testTheStripePointWalksTheCubic() {
        let start = FeedbackStates.Mark.stripePoint(0, t: 0)
        XCTAssertEqual(start.x, -5.87, accuracy: 1e-9)
        XCTAssertEqual(start.y, 62.01, accuracy: 1e-9)
        let end = FeedbackStates.Mark.stripePoint(2, t: 1)
        XCTAssertEqual(end.x, 96.98, accuracy: 1e-9)
        XCTAssertEqual(end.y, -3.98, accuracy: 1e-9)
        XCTAssertEqual(FeedbackStates.Mark.phase(elapsed: 1.8), 0.5, accuracy: 1e-9)
    }

    func testTheInlinePendingAndPlaceholderConstants() throws {
        let inline = try block("inline")
        XCTAssertEqual(FeedbackStates.Inline.spinner, (inline["spinner"] as? NSNumber)?.doubleValue)
        XCTAssertEqual(FeedbackStates.Inline.gap, (inline["gap"] as? NSNumber)?.doubleValue)
        let words = try XCTUnwrap(inline["words"] as? [String: String])
        XCTAssertEqual(FeedbackStates.Inline.saving, words["saving"])
        XCTAssertEqual(FeedbackStates.Inline.saved, words["saved"])
        XCTAssertEqual(FeedbackStates.Inline.loading, words["loading"])

        let pending = try block("pending")
        XCTAssertEqual(FeedbackStates.Pending.delayMs, pending["delayMs"] as? Int)
        XCTAssertEqual(FeedbackStates.Pending.spinner, (pending["spinner"] as? NSNumber)?.doubleValue)
        let verbs = try XCTUnwrap(pending["words"] as? [String: String])
        XCTAssertEqual(FeedbackStates.Pending.start, verbs["start"])
        XCTAssertEqual(FeedbackStates.Pending.connect, verbs["connect"])
        XCTAssertEqual(FeedbackStates.Pending.save, verbs["save"])
        XCTAssertEqual(FeedbackStates.Pending.delete, verbs["delete"])
        XCTAssertEqual(FeedbackStates.Pending.send, verbs["send"])
        XCTAssertEqual(FeedbackStates.Pending.merge, verbs["merge"])
        XCTAssertEqual(FeedbackStates.Pending.invite, verbs["invite"])

        let placeholder = try block("placeholder")
        XCTAssertEqual(FeedbackStates.Placeholder.rows, placeholder["rows"] as? Int)
        XCTAssertEqual(FeedbackStates.Placeholder.barHeight, (placeholder["barHeight"] as? NSNumber)?.doubleValue)
        XCTAssertEqual(FeedbackStates.Placeholder.radius, (placeholder["radius"] as? NSNumber)?.doubleValue)
        XCTAssertEqual(FeedbackStates.Placeholder.dot, (placeholder["dot"] as? NSNumber)?.doubleValue)
        XCTAssertEqual(FeedbackStates.Placeholder.pulseMs, placeholder["pulseMs"] as? Int)
        let opacity = try XCTUnwrap(placeholder["opacity"] as? [NSNumber])
        XCTAssertEqual(FeedbackStates.Placeholder.opacity.low, opacity[0].doubleValue)
        XCTAssertEqual(FeedbackStates.Placeholder.opacity.high, opacity[1].doubleValue)
        XCTAssertEqual(
            FeedbackStates.Placeholder.reducedOpacity,
            (placeholder["reducedOpacity"] as? NSNumber)?.doubleValue
        )
    }

    func testTheBlockingStartingAndErrorCopy() throws {
        let blocking = try block("blocking")
        XCTAssertEqual(FeedbackStates.Blocking.backdropOpacity, (blocking["backdropOpacity"] as? NSNumber)?.doubleValue)
        XCTAssertEqual(FeedbackStates.Blocking.markSize, (blocking["markSize"] as? NSNumber)?.doubleValue)
        XCTAssertEqual(FeedbackStates.Blocking.gap, (blocking["gap"] as? NSNumber)?.doubleValue)
        let copy = try XCTUnwrap(blocking["copy"] as? [String: String])
        XCTAssertEqual(
            FeedbackStates.Blocking.switchTeam("Acme"),
            copy["switchTeam"]?.replacingOccurrences(of: "{team}", with: "Acme")
        )
        XCTAssertEqual(
            FeedbackStates.Blocking.switchAccount("Personal"),
            copy["switchAccount"]?.replacingOccurrences(of: "{account}", with: "Personal")
        )
        XCTAssertEqual(FeedbackStates.Blocking.signOut, copy["signOut"])

        let starting = try block("starting")
        XCTAssertEqual(
            FeedbackStates.Starting.label(device: "macbook"),
            (starting["label"] as? String)?.replacingOccurrences(of: "{device}", with: "macbook")
        )
        XCTAssertEqual(FeedbackStates.Starting.timeoutMs, starting["timeoutMs"] as? Int)
        XCTAssertEqual(FeedbackStates.Starting.watchMs, starting["watchMs"] as? Int)
        XCTAssertEqual(FeedbackStates.Starting.dedupeMs, starting["dedupeMs"] as? Int)
        let toast = try XCTUnwrap(starting["timeoutToast"] as? [String: String])
        XCTAssertEqual(FeedbackStates.Starting.timeoutTitle, toast["title"])
        XCTAssertEqual(
            FeedbackStates.Starting.timeoutDescription(device: "macbook"),
            toast["description"]?.replacingOccurrences(of: "{device}", with: "macbook")
        )
        XCTAssertEqual(FeedbackStates.Starting.timeoutAction, toast["action"])

        let errors = try block("errors")
        XCTAssertEqual(FeedbackStates.Errors.retry, errors["retry"] as? String)
        // EXP-1391 + EXP-1378: only the check gate's code offers `Merge anyway`.
        let refusal = try XCTUnwrap(errors["mergeRefusal"] as? [String: Any])
        XCTAssertEqual(FeedbackStates.Errors.MergeRefusal.checksCode, refusal["checksCode"] as? String)
        XCTAssertEqual(FeedbackStates.Errors.MergeRefusal.force, refusal["force"] as? String)
        XCTAssertEqual(FeedbackStates.Errors.MergeRefusal.handoff, refusal["handoff"] as? String)
        XCTAssertTrue(FeedbackStates.Errors.MergeRefusal.offersForce(code: "PRECONDITION_REQUIRED"))
        for code in ["CONFLICT", "PRECONDITION_FAILED", "FORBIDDEN", nil] as [String?] {
            XCTAssertFalse(FeedbackStates.Errors.MergeRefusal.offersForce(code: code))
        }
        let cases = try XCTUnwrap(errors["cases"] as? [[String: Any]])
        for c in cases where c["kind"] as? String == "action" {
            let verb = try XCTUnwrap(c["verb"] as? String)
            let thing = try XCTUnwrap(c["thing"] as? String)
            XCTAssertEqual(FeedbackStates.Errors.title(verb: verb, thing: thing), c["title"] as? String)
        }
        // Every case renders where the fixture says; background work is quiet.
        for c in cases {
            let kind = try XCTUnwrap(c["kind"] as? String)
            XCTAssertEqual(FeedbackStates.Errors.surface(kind: kind), c["render"] as? String, "\(c["name"] ?? kind)")
        }
        XCTAssertTrue(cases.contains { $0["kind"] as? String == "background" })
        XCTAssertEqual(FeedbackStates.Errors.surface(kind: "background"), "quiet")
    }

    func testTheTestIdsAreTheFixtures() throws {
        let ids = try XCTUnwrap(try fixture()["testIds"] as? [String: String])
        XCTAssertEqual(FeedbackStates.TestId.spinner, ids["spinner"])
        XCTAssertEqual(FeedbackStates.TestId.pending, ids["pending"])
        XCTAssertEqual(FeedbackStates.TestId.skeleton, ids["skeleton"])
        XCTAssertEqual(FeedbackStates.TestId.mark, ids["mark"])
        XCTAssertEqual(FeedbackStates.TestId.blocking, ids["blocking"])
        XCTAssertEqual(FeedbackStates.TestId.starting, ids["starting"])
    }
}
