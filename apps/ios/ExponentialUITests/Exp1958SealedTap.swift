// EXP-1958 LOCAL CHECK ONLY — never commit.
import XCTest

final class Exp1958SealedTap: XCTestCase {
    @MainActor
    func testSealedCardTaps() throws {
        continueAfterFailure = true
        let env = ProcessInfo.processInfo.environment
        let app = XCUIApplication()
        app.launchArguments = ["-uiTesting", "-uiTestingReset"]
        app.launchEnvironment["EXP_UITEST_SESSION"] = env["EXP1958_SESSION"] ?? ""
        app.launch()

        func shot(_ name: String) {
            let png = XCUIScreen.main.screenshot().pngRepresentation
            try? png.write(to: URL(fileURLWithPath: "/tmp/exp1958/\(name).png"))
            let a = XCTAttachment(data: png, uniformTypeIdentifier: "public.png")
            a.name = name
            a.lifetime = .keepAlways
            add(a)
        }

        let field = app.descendants(matching: .any).matching(identifier: "agent-composer-field").firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 60), "no home composer")
        sleep(3)
        for dy in [0.25, 0.5, 0.15] where !app.keyboards.firstMatch.exists {
            field.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: dy)).tap()
            _ = app.keyboards.firstMatch.waitForExistence(timeout: 5)
        }
        app.typeText("EXP-1958 sealed card check")
        sleep(1)
        shot("01-typed")
        let submit = app.descendants(matching: .any).matching(identifier: "agent-composer-submit").firstMatch
        XCTAssertTrue(submit.waitForExistence(timeout: 10), "no submit")
        submit.tap()
        sleep(8)
        shot("02-sent")

        // The host seeds the sealed cards into the new root (seed-sealed-suggest.ts).
        let chip = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Yes, ship it")).firstMatch
        let deadline = Date().addingTimeInterval(600)
        while !chip.exists && Date() < deadline { sleep(2) }
        shot("03-cards")
        XCTAssertTrue(chip.exists, "the sealed replies chip never showed")
        chip.tap()
        sleep(8)
        shot("04-after-chip")

        let start = app.buttons.matching(NSPredicate(format: "label == %@", "Start")).firstMatch
        let startDeadline = Date().addingTimeInterval(60)
        while !start.exists && Date() < startDeadline {
            app.swipeUp()
            sleep(2)
        }
        XCTAssertTrue(start.exists, "no Start on the sealed suggestions card")
        start.tap()
        sleep(10)
        shot("05-after-start")
        sleep(5)
        shot("06-final")
    }
}
