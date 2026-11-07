// #VAPP-268: the iOS runner's glue against the REAL Engine (the workspace's
// eengine-ffi, macOS slice) and packages/eengine/fixtures: runner.json's
// config, failure and toast cases, the bundle reading, and one app run end
// to end (launch, the shell for both form factors, writes, navigation, the
// consent round trip, Permissions, deep links).
//   swift test --package-path apps/runner/ios
import Foundation
import XCTest
import ExponentialEngine
@testable import RunnerKit

let repo = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()

func fixture(_ name: String) -> [String: Any] {
    RunnerJSON.object(try! String(contentsOf: repo.appendingPathComponent("packages/eengine/fixtures/\(name)"), encoding: .utf8))!
}

func same(_ a: Any?, _ b: Any?) -> Bool {
    RunnerJSON.text(a ?? NSNull()) == RunnerJSON.text(b ?? NSNull())
}

final class Recorder: RunnerPainter {
    private let lock = NSLock()
    private var _log: [(String, String, String?)] = []
    var log: [(String, String, String?)] { lock.withLock { _log } }
    func add(_ kind: String, _ a: String, _ b: String? = nil) { lock.withLock { _log.append((kind, a, b)) } }

    func install(packageJson: String) { add("install", packageJson) }
    func open(screen: String, paramsJson: String) { add("open", screen, paramsJson) }
    func paint(surface: String, componentsJson: String, dataJson: String?) { add("paint:\(surface)", componentsJson, dataJson) }
    func close(surface: String) { add("close", surface) }
    func recordsChanged() { add("records", "") }

    func last(_ kind: String) -> (String, String, String?)? { log.last { $0.0 == kind } }
    var opened: [String] { log.filter { $0.0 == "open" }.map(\.1) }
}

/// A photos capability answering an empty pick (capabilities.json `nativeOutputSchema`).
final class FakePhotos: Capability, @unchecked Sendable {
    func name() -> String { "photos" }
    func toolsJson() -> String { "[]" }
    func call(tool: String, argumentsJson: String) throws -> String { #"{"structuredContent":{"photos":[]}}"# }
}

let samplePackage: [String: Any] = [
    "formatVersion": 6, "id": "acme.album", "name": "Album", "version": "1.0.0",
    "catalogId": "https://ui.exponential.at/catalogs/core/v1",
    "functions": ["records_*", "app_navigate", "photos_pick"], "sources": ["records:*"],
    "records": ["note": ["title": "Note", "schema": ["type": "object", "properties": ["text": ["type": "string"]]]]],
    "capabilities": [["id": "photos.read", "why": "Pick the photos for an album."]],
    "screens": [["template": "albums", "title": "Albums", "icon": "image"], ["template": "pages", "title": "Pages"],
                ["template": "page", "title": "Page", "nav": false, "exposed": ["description": "One page of an album."]]],
    "templates": [
        "albums": ["components": [["id": "root", "component": "Text", "text": "Albums"]]],
        "pages": ["components": [["id": "root", "component": "Text", "text": "Pages"]]],
        "page": ["components": [["id": "root", "component": "Text", "text": "Page"]]],
    ],
]

let sampleConfig = RunnerConfig(id: "at.acme.album", name: "Album", version: "1.0.0", network: "off")

func components(_ json: String?) -> [[String: Any]] { (RunnerJSON.value(json ?? "[]") as? [[String: Any]]) ?? [] }
func templateRoot(_ packageJson: String, _ template: String) -> String? {
    let templates = RunnerJSON.object(packageJson)?["templates"] as? [String: Any]
    let comps = (templates?[template] as? [String: Any])?["components"] as? [[String: Any]] ?? []
    return comps.first { $0["id"] as? String == "root" }?["component"] as? String
}

final class RunnerKitTests: XCTestCase {
    func session(_ bundle: Result<RunnerBundle, RunnerFailureKind>, _ painter: Recorder, formFactor: RunnerFormFactor = .compact,
                 vault: Vault = MemoryVault(), dataRoot: URL? = nil) -> RunnerSession {
        RunnerSession(bundle: bundle, dataRoot: dataRoot, vault: vault, capabilities: [FakePhotos()],
                      host: .init(platform: "ios", formFactor: "phone", locale: "en", zone: "UTC"),
                      formFactor: formFactor, painter: painter, deliver: { $0() })
    }

    func running(_ painter: Recorder, formFactor: RunnerFormFactor = .compact, package: [String: Any] = samplePackage) -> RunnerSession {
        let s = session(.success(RunnerBundle(config: sampleConfig, packageJson: RunnerJSON.text(package))), painter, formFactor: formFactor)
        s.start()
        s.settle()
        return s
    }

    // runner.json configCases through RunnerBundle.parseConfig.
    func testConfigCases() {
        for c in fixture("runner.json")["configCases"] as! [[String: Any]] {
            let got = RunnerBundle.parseConfig(RunnerJSON.text(c["input"]!))
            let expected = (c["expected"] as! [String: Any])["config"] as? [String: Any]
            let shaped = got.map { ["id": $0.id, "name": $0.name, "version": $0.version, "network": $0.network] }
            XCTAssert(same(shaped, expected), "\(c["name"]!): \(String(describing: got))")
        }
        XCTAssertNil(RunnerBundle.parseConfig("{"), "not JSON = config")
        XCTAssertEqual(sampleConfig.networkPolicy, .off)
        XCTAssertEqual(RunnerConfig(id: "a.b", name: "n", version: "1.0.0", network: "upstreamsOnly").networkPolicy, .upstreamsOnly)
    }

    func testBundleReading() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("runner-bundle-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        XCTAssertEqual(RunnerBundle.read(nil).failure, .missing)
        XCTAssertEqual(RunnerBundle.read(dir).failure, .config, "no runner.json")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try #"{"id":"album","name":"Album","version":"1.0.0"}"#.write(to: dir.appendingPathComponent("runner.json"), atomically: true, encoding: .utf8)
        XCTAssertEqual(RunnerBundle.read(dir).failure, .config, "an invalid id")
        try #"{"id":"at.acme.album","name":"Album","version":"1.0.0","telemetry":true}"#.write(to: dir.appendingPathComponent("runner.json"), atomically: true, encoding: .utf8)
        XCTAssertEqual(RunnerBundle.read(dir).failure, .config, "a forbidden key")
        try #"{"id":"at.acme.album","name":"Album","version":"1.0.0"}"#.write(to: dir.appendingPathComponent("runner.json"), atomically: true, encoding: .utf8)
        XCTAssertEqual(RunnerBundle.read(dir).failure, .missing, "no package file")
        try RunnerJSON.text(samplePackage).write(to: dir.appendingPathComponent("package.app.json"), atomically: true, encoding: .utf8)
        let bundle = try RunnerBundle.read(dir).get()
        XCTAssertEqual(bundle.config, sampleConfig)
        XCTAssertEqual(RunnerBundle.dir, (fixture("runner.json")["bundle"] as! [String: Any])["dir"] as? String)
        let files = (fixture("runner.json")["bundle"] as! [String: Any])["files"] as! [String: String]
        XCTAssertEqual(RunnerBundle.packageFile, files["package"])
        XCTAssertEqual(RunnerBundle.configFile, files["config"])
    }

    // runner.json failureCases: the failure paints in surface `app`.
    func testFailures() {
        for c in fixture("runner.json")["failureCases"] as! [[String: Any]] {
            let painter = Recorder()
            let s = session(.failure(RunnerFailureKind(rawValue: c["kind"] as! String)!), painter)
            s.start()
            s.settle()
            XCTAssert(same(components(painter.last("paint:app")?.1), c["expected"]), "\(c["kind"]!)")
        }
        // The Engine's own refusals: a bad signature, an invalid package.
        for (file, kind) in [
            (RunnerJSON.text(["package": samplePackage, "author": "ed25519:AAAA", "signature": "AAAA"]), RunnerFailureKind.signature),
            (#"{"formatVersion":4}"#, .invalid),
        ] {
            let painter = Recorder()
            let s = session(.success(RunnerBundle(config: sampleConfig, packageJson: file)), painter)
            s.start()
            s.settle()
            XCTAssertEqual(s.failure, kind, file)
            XCTAssertEqual(components(painter.last("paint:app")?.1).first?["component"] as? String, "EmptyState")
        }
    }

    func testToastCases() {
        for c in fixture("runner.json")["toastCases"] as! [[String: Any]] {
            XCTAssert(same(components(runnerToast(message: c["message"] as! String)), c["expected"]))
        }
        XCTAssertEqual(RunnerCopy.refusedLink, (fixture("lifecycle.json")["copy"] as! [String: Any])["refused"] as? String)
        XCTAssertEqual(RunnerDevice.key, (fixture("runner.json")["storage"] as! [String: Any])["deviceKey"] as? String)
    }

    // Every app-callable catalog tool is routed to the instance.
    func testFunctionsCoverTheCatalog() {
        var names: [String] = []
        for cap in fixture("capabilities.json")["capabilities"] as! [[String: Any]] {
            for scope in cap["scopes"] as! [[String: Any]] where scope["hostOnly"] as? Bool != true {
                names += (scope["tools"] as! [String]).map { "\(cap["name"]!)_\($0)" }
            }
        }
        XCTAssertEqual(Set(RunnerFunctions.capabilities), Set(names))
    }

    // No telemetry, no diagnostics (runner.json): the runner's own sources
    // (RunnerKit + the app template) carry no log, crash or analytics sink
    // and never reach the network themselves.
    func testNoDiagnosticSinks() throws {
        let ios = repo.appendingPathComponent("apps/runner/ios")
        let banned = ["print(", "NSLog(", "os_log(", "Logger(", "OSLog", "URLSession", "MetricKit", "Crashlytics", "Analytics", "debugPrint("]
        var files = 0
        for dir in ["Sources/RunnerKit", "App"] {
            for name in try FileManager.default.contentsOfDirectory(atPath: ios.appendingPathComponent(dir).path) where name.hasSuffix(".swift") {
                let text = try String(contentsOf: ios.appendingPathComponent("\(dir)/\(name)"), encoding: .utf8)
                files += 1
                for word in banned { XCTAssertFalse(text.contains(word), "\(dir)/\(name) uses \(word)") }
            }
        }
        XCTAssertGreaterThan(files, 4)
    }

    // The committed sample bundle (the build check and the screenshots) runs.
    func testSampleBundle() throws {
        let bundle = try RunnerBundle.read(repo.appendingPathComponent("apps/runner/ios/sample/app")).get()
        let painter = Recorder()
        let s = session(.success(bundle), painter)
        s.start()
        s.settle()
        XCTAssertNil(s.failure)
        XCTAssertEqual(painter.opened, ["notes"])
        XCTAssertTrue(painter.last("install")!.1.contains("runner.permissions"))
    }

    func testLaunchAndShell() {
        let painter = Recorder()
        let s = running(painter)
        XCTAssertNil(s.failure)
        let installed = painter.last("install")!.1
        XCTAssertEqual(templateRoot(installed, "albums"), "Stack", "compact = the Tabs shell")
        XCTAssertTrue(installed.contains("runner_tabs") && !installed.contains(#""component":"Sidebar""#))
        XCTAssertTrue(installed.contains("runner.permissions"), "an ask scope = the Permissions entry")
        XCTAssertEqual(painter.opened, ["albums"], "launch opens the first navigation screen")
        // Wider than md: re-install the Sidebar shell and re-apply the open screen.
        s.setFormFactor(.wide)
        s.settle()
        let wide = painter.last("install")!.1
        XCTAssertTrue(wide.contains(#""component":"Sidebar""#), "wide = the Sidebar shell")
        XCTAssertTrue(wide.contains("runner_scroll") && !wide.contains("runner_tabs"))
        XCTAssertEqual(painter.opened, ["albums", "albums"])
        // A package without screens gets one: the plain launch opens it.
        var bare = samplePackage
        bare["screens"] = nil
        bare["capabilities"] = nil
        bare["functions"] = ["records.*"]
        bare["templates"] = ["only": ["components": [["id": "root", "component": "Text", "text": "Only"]]]]
        let painter2 = Recorder()
        _ = running(painter2, package: bare)
        XCTAssertEqual(painter2.opened, ["only"])
    }

    func testCallsNavigationAndConsent() async {
        let painter = Recorder()
        let s = running(painter)
        // records.* through the instance; a write repaints the bound sources.
        let created = await s.call(name: "records_upsert", argsJson: #"{"kind":"note","data":{"text":"hi"}}"#)
        guard case let .value(json) = created else { return XCTFail("upsert refused: \(created)") }
        XCTAssertNotNil(RunnerJSON.object(json)?["id"] as? String)
        XCTAssertNotNil(painter.last("records"))
        let value = await withCheckedContinuation { c in s.recordsValue(source: "records:note") { c.resume(returning: $0) } }
        XCTAssertEqual((RunnerJSON.object(value ?? "{}")?["count"] as? NSNumber)?.intValue, 1)
        // runner.navigate = app.navigate.
        XCTAssertTrue(s.handleEvent(name: "runner.navigate", context: ["screen": "pages"]))
        s.settle()
        XCTAssertEqual(painter.opened.last, "pages")
        XCTAssertFalse(s.handleEvent(name: "acme.refresh", context: [:]), "app events are not the runner's")
        // An undeclared capability: refused, the toast paints.
        let refused = await s.call(name: "camera_capture", argsJson: "{}")
        guard case .refused = refused else { return XCTFail("camera.capture ran") }
        XCTAssertEqual(components(painter.last("paint:toast")?.1).first?["component"] as? String, "Toast")
        // An ask scope: the consent sheet paints, the answer runs the call.
        async let picked = s.call(name: "photos_pick", argsJson: "{}")
        while painter.last("paint:consent") == nil { try? await Task.sleep(nanoseconds: 5_000_000) }
        XCTAssertEqual(components(painter.last("paint:consent")?.1).first?["component"] as? String, "Drawer")
        s.handleEvent(name: "consent.answer", context: ["answer": "allow"])
        guard case let .value(result) = await picked else { return XCTFail("allow did not run the pick") }
        XCTAssertTrue(result.contains("photos"), result)
        XCTAssertEqual(painter.last("close")?.1, "consent")
        XCTAssertEqual(RunnerJSON.object(s.instance!.grantsJson())?["photos.read"] as? String, "granted")
        // Permissions: one Switch per ask scope; a toggle revokes the grant.
        s.handleEvent(name: "runner.permissions", context: [:])
        s.settle()
        let sheet = components(painter.last("paint:permissions")?.1)
        XCTAssertEqual(sheet.first?["title"] as? String, "Permissions")
        XCTAssertEqual(sheet.first { $0["component"] as? String == "Switch" }?["checked"] as? Bool, true)
        s.handleEvent(name: "consent.toggle", context: ["scope": "photos.read"])
        s.settle()
        XCTAssertNil(RunnerJSON.object(s.instance!.grantsJson())?["photos.read"] as? String)
        XCTAssertEqual(components(painter.last("paint:permissions")?.1).first { $0["component"] as? String == "Switch" }?["checked"] as? Bool, false)
        s.handleEvent(name: "consent.toggle", context: ["scope": "photos.read"])
        s.settle()
        XCTAssertEqual(RunnerJSON.object(s.instance!.grantsJson())?["photos.read"] as? String, "granted")
        // A dismissed sheet refuses the call.
        try! s.instance!.setGrant(scope: "photos.read", value: nil)
        async let dismissed = s.call(name: "photos_pick", argsJson: "{}")
        while painter.log.filter({ $0.0 == "paint:consent" }).count < 2 { try? await Task.sleep(nanoseconds: 5_000_000) }
        s.handleEvent(name: "consent.answer", context: ["answer": "dismiss"])
        guard case .refused = await dismissed else { return XCTFail("a dismissed sheet ran the call") }
    }

    func testDeepLinksAndScenes() {
        XCTAssertEqual(RunnerLinks.event(URL(string: "at.acme.album://page?id=7")!, id: "at.acme.album"), #"{"params":{"id":"7"},"screen":"page","type":"deepLink"}"#)
        XCTAssertNil(RunnerLinks.event(URL(string: "https://example.com/page")!, id: "at.acme.album"))
        XCTAssertNil(RunnerLinks.event(URL(string: "at.acme.album://")!, id: "at.acme.album"))
        let painter = Recorder()
        let s = running(painter)
        XCTAssertFalse(s.open(url: URL(string: "other://page")!))
        // An unknown screen is refused: the toast, the screen stays.
        XCTAssertTrue(s.open(url: URL(string: "at.acme.album://nowhere")!))
        s.settle()
        XCTAssertEqual(components(painter.last("paint:toast")?.1).first?["title"] as? String, RunnerCopy.refusedLink)
        XCTAssertEqual(painter.opened, ["albums"])
        // An exposed screen opens.
        XCTAssertTrue(s.open(url: URL(string: "at.acme.album://page")!))
        s.settle()
        XCTAssertEqual(painter.opened, ["albums", "page"])
        s.scene(.background)
        s.scene(.foreground)
        s.settle()
        XCTAssertEqual(painter.opened, ["albums", "page"], "foreground keeps the screen")
    }

    // The device uuid is created once; the default stores persist under dataRoot/<id>.
    func testDeviceAndStorage() async {
        let vault = MemoryVault()
        let id = RunnerDevice.id(vault)
        XCTAssertEqual(RunnerDevice.id(vault), id)
        XCTAssertEqual(id.count, 36)
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("runner-data-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        for round in 0..<2 {
            let s = session(.success(RunnerBundle(config: sampleConfig, packageJson: RunnerJSON.text(samplePackage))), Recorder(), vault: vault, dataRoot: root)
            s.start()
            s.settle()
            if round == 0 { _ = await s.call(name: "records_upsert", argsJson: #"{"kind":"note","data":{}}"#) }
            let value = await withCheckedContinuation { c in s.recordsValue(source: "records:note") { c.resume(returning: $0) } }
            XCTAssertEqual((RunnerJSON.object(value ?? "{}")?["count"] as? NSNumber)?.intValue, 1, "round \(round)")
        }
        XCTAssertTrue(FileManager.default.fileExists(atPath: root.appendingPathComponent("at.acme.album").path))
    }

    // VAPP-271: an `app-file:<fileId>` image reads the app's files: the store the Engine was handed.
    func testAppFilesReadTheEnginesStore() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("runner-files-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let idle = session(.success(RunnerBundle(config: sampleConfig, packageJson: RunnerJSON.text(samplePackage))), Recorder(), dataRoot: root)
        let notRunning = await idle.appFile("3f2c9a1e-8b7d-4c1e-9f00-1a2b3c4d5e6f")
        XCTAssertNil(notRunning, "not running: nothing")
        for dataRoot in [root, nil] as [URL?] {
            let s = session(.success(RunnerBundle(config: sampleConfig, packageJson: RunnerJSON.text(samplePackage))), Recorder(), dataRoot: dataRoot)
            s.start()
            s.settle()
            let blobs = try XCTUnwrap(s.blobs)
            let id = try blobs.put(app: "acme.album", name: "a.png", mime: "image/png", bytes: Data([1, 2, 3]))
            let got = await s.appFile(id)
            XCTAssertEqual(got, Data([1, 2, 3]), "\(String(describing: dataRoot))")
            let missing = await s.appFile(UUID().uuidString.lowercased())
            XCTAssertNil(missing)
            let notAnId = await s.appFile("../a")
            XCTAssertNil(notAnId)
            if dataRoot != nil {
                // The Engine's FsBlobStore layout: <dataDir>/blobs/<app>/<id> + <id>.json.
                let dir = root.appendingPathComponent("at.acme.album/blobs/acme.album")
                XCTAssertEqual(try Data(contentsOf: dir.appendingPathComponent(id)), Data([1, 2, 3]))
                let meta = RunnerJSON.object(try String(contentsOf: dir.appendingPathComponent("\(id).json"), encoding: .utf8))
                XCTAssertEqual(meta?["mime"] as? String, "image/png")
                try blobs.delete(app: "acme.album", id: id)
                let deleted = await s.appFile(id)
                XCTAssertNil(deleted)
            }
        }
    }
}

extension Result {
    var failure: Failure? { if case let .failure(f) = self { return f } else { return nil } }
}
