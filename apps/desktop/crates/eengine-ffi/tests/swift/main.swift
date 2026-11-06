// #VAPP-263: the Swift binding suite. Runs an app through the facade with
// Swift providers (the lifecycle, writes, the consent round trip, the
// default SQLite store) and replays packages/eengine/fixtures/providers.json
// `validateCases`.
//   bash apps/desktop/crates/eengine-ffi/run-binding-tests.sh swift
import Foundation

var failures = 0
var checks = 0

func check(_ ok: Bool, _ message: @autoclosure () -> String) {
    checks += 1
    if !ok {
        failures += 1
        print("FAIL: \(message())")
    }
}

func json(_ text: String) -> Any { try! JSONSerialization.jsonObject(with: Data(text.utf8), options: [.fragmentsAllowed]) }
func text(_ value: Any) -> String { String(data: try! JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .fragmentsAllowed]), encoding: .utf8)! }

final class Keychain: Vault, @unchecked Sendable {
    var values: [String: String] = [:]
    func read(key: String) -> String? { values[key] }
    func write(key: String, value: String) throws { values[key] = value }
    func delete(key: String) throws { values[key] = nil }
}

final class Nav: WindowHost, @unchecked Sendable {
    var opened: [String] = []
    func present(app: String, screen: String, paramsJson: String, how: Presentation) { opened.append(screen) }
}

final class Photos: Capability, @unchecked Sendable {
    func name() -> String { "photos" }
    func toolsJson() -> String { #"[{"name":"pick","inputSchema":{"type":"object"}}]"# }
    func call(tool: String, argumentsJson: String) throws -> String {
        text(["content": [["type": "text", "text": "\(tool): p1"]]])
    }
}

final class Pinned: EngineClock, @unchecked Sendable {
    func now() -> String { "2026-11-06T10:00:00.000Z" }
}

let package = #"""
{"formatVersion":5,"id":"acme.album","name":"Album","version":"1.0.0","catalogId":"https://ui.exponential.at/catalogs/core/v1",
 "functions":["records.*","app.navigate","photos.pick"],"sources":["records:*"],
 "records":{"note":{"title":"Note","schema":{"type":"object","properties":{"text":{"type":"string"}}},"policy":{"update":"author","delete":"owners"}}},
 "capabilities":[{"id":"photos.read","why":"Pick the photos for an album."}],
 "screens":[{"template":"albums","title":"Albums"},{"template":"page","title":"Page","nav":false}],
 "templates":{"albums":{"components":[{"id":"root","component":"Text","text":"Albums"}]},"page":{"components":[{"id":"root","component":"Text","text":"Page"}]}}}
"""#

func host(_ dataDir: String?) -> EngineHost {
    EngineHost(kind: "exported", platform: "ios", formFactor: "phone", locale: "en", zone: nil, network: .off, dataDir: dataDir)
}

func count(_ app: AppInstance) -> Int {
    guard let value = app.recordsValue(source: "records:note") else { return -1 }
    return ((json(value) as! [String: Any])["count"] as! NSNumber).intValue
}

// An app runs over the default stores.
let keychain = Keychain()
let nav = Nav()
let engine = try! Engine(host: host(nil), identity: .local(deviceId: "d-1"), providers: EngineProviders(vault: keychain, window: nav, capabilities: [Photos()], clock: Pinned()))
let app = try! engine.load(fileJson: package)
check(app.appId() == "acme.album", "appId")
check(!app.queues(), "no sync provider: writes go to the store")
let effects = json(try! app.event(eventJson: #"{"type":"launch"}"#)) as! [[String: Any]]
check(effects.contains { $0["effect"] as? String == "flush" }, "launch flushes: \(effects)")
check(nav.opened.last == "albums", "launch opens the first screen: \(nav.opened)")
if case let .value(created) = try! app.call(name: "records.upsert", argsJson: #"{"kind":"note","data":{"text":"hi"}}"#, newIds: ["n1"]) {
    check(created == #"{"id":"n1"}"#, "upsert answers the id: \(created)")
} else { check(false, "upsert refused") }
check(count(app) == 1, "the record is there")
_ = try! app.call(name: "app.navigate", argsJson: #"{"screen":"page"}"#, newIds: [])
check(nav.opened.last == "page", "navigate opens the screen")
do { _ = try app.event(eventJson: "{"); check(false, "bad JSON throws") } catch EngineError.Invalid { check(true, "") } catch { check(false, "\(error)") }

// The consent round trip.
guard case let .ask(verdict, sheet) = try! app.call(name: "photos.pick", argsJson: "{}", newIds: []) else { fatalError("no sheet") }
check(sheet.scope == "photos.read", "the sheet's scope")
let title = ((json(sheet.componentsJson) as! [[String: Any]])[0]["title"] as? String) ?? ""
check(title == "Allow Album to access your photos?", "the sheet's title: \(title)")
if case let .value(picked) = try! app.answerConsent(name: "photos.pick", argsJson: "{}", verdictJson: verdict, answer: .allow) {
    check(picked.contains("pick: p1"), "the capability ran: \(picked)")
} else { check(false, "allow did not run the call") }
check(app.grantsJson() == #"{"photos.read":"granted"}"#, "the grant persists: \(app.grantsJson())")
check(keychain.values["eengine/grants/acme.album"]?.contains("granted") == true, "in the Vault")
try! app.setGrant(scope: "photos.read", value: nil)
if case .ask = try! app.call(name: "photos.pick", argsJson: "{}", newIds: []) { check(true, "") } else { check(false, "revoked: asks again") }
if case let .refused(_, message) = try! app.call(name: "camera.capture", argsJson: "{}", newIds: []) {
    check(message == "notDeclared", "undeclared: \(message)")
} else { check(false, "camera.capture ran") }

// A bad package says why.
do { _ = try engine.load(fileJson: #"{"formatVersion":4}"#); check(false, "format 4 loaded") }
catch let EngineError.Load(kind, _) { check(kind == "invalid", "load kind \(kind)") } catch { check(false, "\(error)") }

// The default SQLite store keeps records across Engines (feature `sqlite`).
if engineFeatures().contains("sqlite") {
    let dir = NSTemporaryDirectory() + "eengine-swift-\(getpid())"
    try? FileManager.default.removeItem(atPath: dir)
    for round in 0..<2 {
        let e = try! Engine(host: host(dir), identity: .local(deviceId: "d-1"), providers: EngineProviders(vault: Keychain(), window: Nav()))
        let a = try! e.load(fileJson: package)
        _ = try! a.event(eventJson: #"{"type":"launch"}"#)
        if round == 0 { _ = try! a.call(name: "records.upsert", argsJson: #"{"kind":"note","data":{}}"#, newIds: ["n1"]) }
        check(count(a) == 1, "round \(round): the record survives")
    }
    try? FileManager.default.removeItem(atPath: dir)
}

// providers.json validateCases.
if CommandLine.arguments.count > 1, let fixtureText = try? String(contentsOfFile: CommandLine.arguments[1], encoding: .utf8) {
    let fixture = json(fixtureText) as! [String: Any]
    for c in fixture["validateCases"] as! [[String: Any]] {
        let got = text(json(try! validateHostConfig(configJson: text(c["config"]!))))
        check(got == text(c["expected"]!), "\(c["name"]!): \(got)")
    }
}

print("\(checks - failures)/\(checks) checks passed (eengine-ffi \(engineVersion()), features \(engineFeatures()))")
exit(failures == 0 ? 0 : 1)
