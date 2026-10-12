import XCTest
@testable import ExpCore

// EXP-1259 start surfaces: one startId per press-intent, reused by that
// press's Retry. Android mirrors these cases (`StartIntentTest.kt`).
final class StartIntentTests: XCTestCase {
    private var counter = 0
    private func mint() -> String {
        counter += 1
        return "id-\(counter)"
    }

    private let target = StartIntent.resumeTarget(sessionId: "s1", deviceId: "mac", account: nil)

    func testAPressMints() {
        XCTAssertEqual(StartIntent.press(target: target, mint: mint).startId, "id-1")
    }

    func testARetryReusesThePressesId() {
        let press = StartIntent.press(target: target, mint: mint)
        let retry = StartIntent.retry(pending: press, target: target, mint: mint)
        XCTAssertEqual(retry, press)
        XCTAssertEqual(StartIntent.retry(pending: retry, target: target, mint: mint).startId, "id-1")
    }

    func testANewPressMintsANewId() {
        let first = StartIntent.press(target: target, mint: mint)
        let second = StartIntent.press(target: target, mint: mint)
        XCTAssertNotEqual(second.startId, first.startId)
        XCTAssertEqual(second.startId, "id-2")
    }

    func testARetryWithNothingPendingMints() {
        XCTAssertEqual(StartIntent.retry(pending: nil, target: target, mint: mint).startId, "id-1")
    }

    func testARetryOfAnotherTargetMints() {
        let press = StartIntent.press(target: target, mint: mint)
        let others = [
            StartIntent.resumeTarget(sessionId: "s2", deviceId: "mac", account: nil),
            StartIntent.resumeTarget(sessionId: "s1", deviceId: "linux", account: nil),
            StartIntent.resumeTarget(sessionId: "s1", deviceId: "mac", account: "profile-2"),
        ]
        for other in others {
            XCTAssertNotEqual(
                StartIntent.retry(pending: press, target: other, mint: mint).startId, press.startId
            )
        }
    }

    func testTheMintedIdIsALowercaseUuid() {
        let id = StartIntent.mint()
        XCTAssertNotNil(UUID(uuidString: id))
        XCTAssertEqual(id, id.lowercased())
    }
}
