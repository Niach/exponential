import XCTest

// TEMP (session dbae3145, never committed): a scripted hand check. The script
// comes from /tmp/dbae-stack/script.txt, one step per line:
//   open <url> | wait <s> | shot <name> | dump <name> | tap <identifier> |
//   tapLabel <label> | type <identifier> <text> | swipeDown <n> | swipeUp <n> | relaunch
final class DbaeProbe: XCTestCase {
    @MainActor
    func testProbe() throws {
        continueAfterFailure = true
        let app = makeScreenshotApp()
        launchSignedIn(app)
        let out = URL(fileURLWithPath: "/tmp/dbae-stack/out")
        try? FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
        let script = (try? String(contentsOfFile: "/tmp/dbae-stack/script.txt", encoding: .utf8)) ?? ""
        var log = ""
        for raw in script.split(separator: "\n") {
            let line = raw.trimmingCharacters(in: .whitespaces)
            guard !line.isEmpty, !line.hasPrefix("#") else { continue }
            let parts = line.split(separator: " ", maxSplits: 2).map(String.init)
            let arg = parts.count > 1 ? parts[1] : ""
            log += "> \(line)\n"
            switch parts[0] {
            case "open":
                app.open(URL(string: arg)!)
            case "wait":
                Thread.sleep(forTimeInterval: Double(arg) ?? 1)
            case "shot":
                let png = XCUIScreen.main.screenshot().pngRepresentation
                try? png.write(to: out.appendingPathComponent("\(arg).png"))
            case "dump":
                try? app.debugDescription.write(to: out.appendingPathComponent("\(arg).txt"), atomically: true, encoding: .utf8)
            case "tap":
                let el = app.descendants(matching: .any)[arg].firstMatch
                if el.waitForExistence(timeout: 8) { el.tap() } else { log += "  missing \(arg)\n" }
            case "tapLabel":
                let rest = parts.dropFirst().joined(separator: " ")
                let el = app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", rest)).firstMatch
                if el.waitForExistence(timeout: 8) { el.tap() } else { log += "  missing label \(rest)\n" }
            case "type":
                let el = app.descendants(matching: .any)[arg].firstMatch
                if el.waitForExistence(timeout: 8) {
                    el.tap()
                    el.typeText(parts.count > 2 ? parts[2] : "")
                } else { log += "  missing \(arg)\n" }
            case "keys":
                app.typeText(parts.dropFirst().joined(separator: " "))
            case "swipeDown":
                for _ in 0..<(Int(arg) ?? 1) { app.swipeDown(velocity: .fast) }
            case "swipeUp":
                for _ in 0..<(Int(arg) ?? 1) { app.swipeUp(velocity: .fast) }
            case "relaunch":
                launchSignedIn(app)
            default:
                log += "  unknown\n"
            }
        }
        try? log.write(to: out.appendingPathComponent("log.txt"), atomically: true, encoding: .utf8)
    }
}
