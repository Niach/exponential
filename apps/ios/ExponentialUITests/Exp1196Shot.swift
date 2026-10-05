import XCTest

final class Exp1196Shot: XCTestCase {
    @MainActor
    func testComputerUseSheet() throws {
        let app = XCUIApplication()
        app.launchArguments += ["-uiTesting"]
        app.launch()
        signIn(app)
        let devicesTab = app.buttons["tab-devices"]
        XCTAssertTrue(devicesTab.waitForExistence(timeout: 60), "Devices tab missing")
        devicesTab.tap()
        let gear = app.buttons["machine-settings"].firstMatch
        XCTAssertTrue(gear.waitForExistence(timeout: 60), "No settings gear")
        gear.tap()
        let toggle = anyElement(app, identified: "device-computer-use")
        XCTAssertTrue(toggle.waitForExistence(timeout: 20), "No computer use toggle")
        let footer = anyElement(app, containing: "stay off limits")
        _ = footer.waitForExistence(timeout: 5)
        if !footer.isHittable { app.swipeUp() }
        sleep(2)
        let png = XCUIScreen.main.screenshot().pngRepresentation
        try? FileManager.default.createDirectory(atPath: "/tmp/exp1196-shots", withIntermediateDirectories: true)
        try png.write(to: URL(fileURLWithPath: "/tmp/exp1196-shots/ios.png"))
        print("EXP1196 footerHittable=\(footer.isHittable)")
    }
}
