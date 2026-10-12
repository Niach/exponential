import XCTest
@testable import ExpCore

// feedback-states.json `starting` (final review): one startId per draft
// start, bound to the device + the draft fingerprint. Android mirrors these
// cases (`DraftStartTest.kt`).
final class DraftStartTests: XCTestCase {
    private let draft = DraftStart.Fingerprint(
        text: "fix the login", issueIds: ["i1"], actionId: nil, attachments: ["a1"]
    )
    private var counter = 0
    private func mint() -> String {
        counter += 1
        return "id-\(counter)"
    }

    func testTheFirstStartMints() {
        let start = DraftStart.resolve(pending: nil, deviceId: "mac", fingerprint: draft, mint: mint)
        XCTAssertEqual(start.startId, "id-1")
        XCTAssertEqual(start.deviceId, "mac")
    }

    func testTheSameDraftOnTheSameDeviceReusesTheId() {
        let first = DraftStart.resolve(pending: nil, deviceId: "mac", fingerprint: draft, mint: mint)
        let again = DraftStart.resolve(pending: first, deviceId: "mac", fingerprint: draft, mint: mint)
        XCTAssertEqual(again, first)
    }

    func testAnotherDeviceMintsAFreshId() {
        let first = DraftStart.resolve(pending: nil, deviceId: "mac", fingerprint: draft, mint: mint)
        let other = DraftStart.resolve(pending: first, deviceId: "linux", fingerprint: draft, mint: mint)
        XCTAssertEqual(other.startId, "id-2")
    }

    func testAChangedDraftMintsAFreshId() {
        let first = DraftStart.resolve(pending: nil, deviceId: "mac", fingerprint: draft, mint: mint)
        let changes = [
            DraftStart.Fingerprint(text: "fix it", issueIds: ["i1"], actionId: nil, attachments: ["a1"]),
            DraftStart.Fingerprint(text: "fix the login", issueIds: ["i1", "i2"], actionId: nil, attachments: ["a1"]),
            DraftStart.Fingerprint(text: "fix the login", issueIds: [], actionId: "act", attachments: ["a1"]),
            DraftStart.Fingerprint(text: "fix the login", issueIds: ["i1"], actionId: nil, attachments: []),
        ]
        for change in changes {
            let next = DraftStart.resolve(pending: first, deviceId: "mac", fingerprint: change, mint: mint)
            XCTAssertNotEqual(next.startId, first.startId)
        }
    }

    func testALandingClearsOnlyAnUnchangedDraft() {
        let start = DraftStart.resolve(pending: nil, deviceId: "mac", fingerprint: draft, mint: mint)
        XCTAssertTrue(start.landsInto(draft))
        XCTAssertFalse(start.landsInto(
            DraftStart.Fingerprint(text: "a new prompt", issueIds: [], actionId: nil, attachments: [])
        ))
    }

    func testTheQuietWatchSpansTheDedupeWindow() {
        XCTAssertEqual(StartedRunMatch.watchWindow, 1800)
        XCTAssertEqual(FeedbackStates.Starting.watchMs, FeedbackStates.Starting.dedupeMs)
    }
}
