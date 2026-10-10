import Foundation
import XCTest
@testable import ExpCore

// F6 (release 2026-10-10): the "Add file or image" pick takes non-image
// files only for a device advertising `steer-files`; images always pass.
final class SteerFilesTests: XCTestCase {
    func testOnlyTheCapAcceptsFiles() {
        XCTAssertTrue(SteerFiles.accepted(caps: ["actions", "steer-files"]))
        XCTAssertFalse(SteerFiles.accepted(caps: ["actions", "resume-run"]))
        XCTAssertFalse(SteerFiles.accepted(caps: []))
    }

    // A row with no caps field at all is an old device, never a lenient one.
    func testNoCapsFieldLacksTheCap() {
        XCTAssertFalse(SteerFiles.accepted(caps: nil))
        XCTAssertFalse(SteerFiles.accepts(contentType: "application/pdf", caps: nil))
    }

    func testImagesPassWithoutTheCapAndFilesNeedIt() {
        XCTAssertTrue(SteerFiles.accepts(contentType: "image/png", caps: nil))
        XCTAssertTrue(SteerFiles.accepts(contentType: "image/jpeg", caps: []))
        XCTAssertFalse(SteerFiles.accepts(contentType: "application/pdf", caps: []))
        XCTAssertFalse(SteerFiles.accepts(contentType: "text/plain", caps: ["actions"]))
        XCTAssertTrue(SteerFiles.accepts(contentType: "application/pdf", caps: ["steer-files"]))
    }

    // The notice is the server's own sentence, so a refused start and the
    // composer's filter read the same; no em dash in it.
    func testTheNoticeIsTheServersSentence() {
        XCTAssertEqual(
            SteerFiles.needsNewerDeviceNotice,
            "Attaching files needs the device on 0.14.66 or newer; images still work"
        )
        XCTAssertFalse(SteerFiles.needsNewerDeviceNotice.contains("—"))
        XCTAssertEqual(SteerFiles.cap, "steer-files")
    }
}
