// #VAPP-271: THE Photobook flow on the iOS runner, end to end on a simulator:
// Albums (empty) → the Permissions sheet → a new album → Add photos (the
// consent sheet, then the system PHPicker) → a page of thumbnails → Export
// PDF. Drives the INSTALLED bundle by its id (check-flow.sh installs it and
// adds the photos with `simctl addmedia`, then checks the PDF in the app's
// container). Screenshots go to $FLOW_SHOTS (TEST_RUNNER_FLOW_SHOTS).
import XCTest

final class PhotobookFlowTests: XCTestCase {
    let env = ProcessInfo.processInfo.environment
    lazy var app = XCUIApplication(bundleIdentifier: env["FLOW_BUNDLE_ID"] ?? "at.exponential.samples.photobook")
    let wait: TimeInterval = 20

    override func setUp() {
        continueAfterFailure = false
    }

    func testPhotobookFlow() throws {
        app.terminate()
        app.launch()

        // Albums, empty.
        XCTAssertTrue(app.staticTexts["No albums yet"].waitForExistence(timeout: wait), dump("no empty Albums"))
        shot("ios-albums-empty")

        // The Permissions sheet (the shell's icon-only button), then close it.
        let permissions = app.buttons["Permissions"]
        XCTAssertTrue(permissions.waitForExistence(timeout: wait), dump("no Permissions button"))
        permissions.tap()
        XCTAssertTrue(app.staticTexts["Pick the photos for an album."].waitForExistence(timeout: wait), dump("no Permissions sheet"))
        sleep(1)
        shot("ios-permissions")
        closeSheet()

        // A new album.
        let title = app.textFields.firstMatch
        XCTAssertTrue(title.waitForExistence(timeout: wait), dump("no title field"))
        title.tap()
        title.typeText("Summer")
        app.buttons["New album"].firstMatch.tap()
        let row = app.descendants(matching: .any)["Summer"].firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: wait), dump("the album did not appear"))
        row.tap()

        // Add photos: the consent sheet (Allow → Continue), then the system picker.
        let add = app.buttons["Add photos"].firstMatch
        XCTAssertTrue(add.waitForExistence(timeout: wait), dump("no album screen"))
        XCTAssertTrue(app.staticTexts["No pages yet"].waitForExistence(timeout: wait), dump("no empty album"))
        add.tap()
        allow()
        pickPhotos(3)

        // A page with the thumbnails (the app-file images loaded).
        XCTAssertTrue(app.staticTexts["No pages yet"].waitForNonExistence(timeout: wait), dump("no page appeared"))
        let thumbnails = app.images.matching(identifier: "Photo")
        XCTAssertTrue(thumbnails.firstMatch.waitForExistence(timeout: wait), dump("no thumbnails"))
        XCTAssertEqual(thumbnails.count, 3, dump("not the 3 picked photos"))
        sleep(2)
        shot("ios-album-pages")

        // Export PDF: the file's name appears with Save and Share.
        app.buttons["Export PDF"].firstMatch.tap()
        allow()
        let pdf = app.staticTexts.matching(NSPredicate(format: "label ENDSWITH[c] '.pdf'")).firstMatch
        XCTAssertTrue(pdf.waitForExistence(timeout: wait), dump("no exported PDF"))
        XCTAssertTrue(app.buttons["Share"].exists && app.buttons["Save"].exists, dump("no Save / Share"))
        shot("ios-album-pdf")
    }

    /// The consent sheet, when the scope asks: Allow, then Continue.
    func allow() {
        let confirm = app.buttons["Continue"].firstMatch
        guard confirm.waitForExistence(timeout: 5) else { return }
        let allow = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Allow'")).firstMatch
        if allow.exists { allow.tap() }
        confirm.tap()
        _ = confirm.waitForNonExistence(timeout: wait)
    }

    /// PHPicker (out of process, reached through the app's tree): the first
    /// `count` images, then Add.
    func pickPhotos(_ count: Int) {
        let images = app.images.matching(identifier: "PXGGridLayout-Info") // the grid cells, newest first
        XCTAssertTrue(images.firstMatch.waitForExistence(timeout: wait), dump("no photo picker"))
        if env["FLOW_DEBUG"] != nil { print("PICKER TREE\n\(app.debugDescription)") }
        for i in 0..<count { images.element(boundBy: i).tap() }
        let done = app.buttons.matching(NSPredicate(format: "label IN {'Add', 'Done'} OR identifier IN {'Add', 'Done'}")).firstMatch
        XCTAssertTrue(done.waitForExistence(timeout: wait), dump("no Add in the picker"))
        done.tap()
    }

    /// The Permissions sheet (a Drawer, an Alert to XCUITest): its scrim, else a swipe down.
    func closeSheet() {
        let text = app.staticTexts["Pick the photos for an album."]
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.3)).tap()
        if !text.waitForNonExistence(timeout: 3) {
            app.alerts["Permissions"].firstMatch.swipeDown(velocity: .fast)
        }
        XCTAssertTrue(text.waitForNonExistence(timeout: wait), dump("the Permissions sheet stays open"))
    }

    func shot(_ name: String) {
        let png = XCUIScreen.main.screenshot().pngRepresentation
        let attachment = XCTAttachment(data: png, uniformTypeIdentifier: "public.png")
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
        if let dir = env["FLOW_SHOTS"] {
            try? png.write(to: URL(fileURLWithPath: dir).appendingPathComponent("\(name).png"))
        }
    }

    /// The failure message plus the app's tree (the run's log).
    func dump(_ message: String) -> String {
        "\(message)\n\(app.debugDescription)"
    }
}
