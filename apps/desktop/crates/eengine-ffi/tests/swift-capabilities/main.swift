// #VAPP-265: the Swift capabilities against fake seams. Replays
// packages/eengine/fixtures/capabilities.json: every native tool is served,
// every `bridgeCases` native call (its `native` arguments) answers its
// `nativeResult`, and every result's structuredContent matches the tool's
// `nativeOutputSchema`; plus cancel/denied/unavailable/main-thread semantics,
// the cursor, scheduling and the tap round trip.
//   bash apps/desktop/crates/eengine-ffi/run-binding-tests.sh swift
import Foundation
import UniformTypeIdentifiers
import ExponentialEngine
@testable import ExponentialEngineCapabilities

setvbuf(stdout, nil, _IONBF, 0)
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
func date(_ s: String) -> Date { RFC3339.parse(s)! }

let fixturePath = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "packages/eengine/fixtures/capabilities.json"
let fixture = json(try! String(contentsOfFile: fixturePath, encoding: .utf8)) as! [String: Any]
let tools = Dictionary(uniqueKeysWithValues: (fixture["tools"] as! [[String: Any]]).map { ($0["name"] as! String, $0) })

// A minimal checker for the keywords nativeOutputSchema uses.
func violations(_ value: Any, _ schema: [String: Any], _ path: String = "") -> [String] {
    let at = path.isEmpty ? "/" : path
    if let type = schema["type"] {
        let types = (type as? [String]) ?? [type as! String]
        let ok = types.contains { t in
            switch t {
            case "null": return value is NSNull
            case "object": return value is [String: Any]
            case "array": return value is [Any]
            case "string": return value is String
            case "boolean": return (value as? NSNumber)?.isBool == true
            case "integer": return (value as? NSNumber).map { !$0.isBool && $0.doubleValue == $0.doubleValue.rounded() } ?? false
            case "number": return (value as? NSNumber).map { !$0.isBool } ?? false
            default: return false
            }
        }
        if !ok { return ["\(at) type"] }
    }
    var out: [String] = []
    if let object = value as? [String: Any] {
        let properties = schema["properties"] as? [String: Any] ?? [:]
        for key in schema["required"] as? [String] ?? [] where object[key] == nil { out.append("\(at) required \(key)") }
        if schema["additionalProperties"] as? Bool == false {
            for key in object.keys.sorted() where properties[key] == nil { out.append("\(path)/\(key) additionalProperties") }
        }
        for (key, sub) in properties { if let v = object[key] { out += violations(v, sub as! [String: Any], "\(path)/\(key)") } }
    }
    if let list = value as? [Any] {
        if let max = schema["maxItems"] as? Int, list.count > max { out.append("\(at) maxItems") }
        if let min = schema["minItems"] as? Int, list.count < min { out.append("\(at) minItems") }
        if let items = schema["items"] as? [String: Any] { for (i, v) in list.enumerated() { out += violations(v, items, "\(path)/\(i)") } }
    }
    if let s = value as? String {
        let n = s.unicodeScalars.count
        if let min = schema["minLength"] as? Int, n < min { out.append("\(at) minLength") }
        if let max = schema["maxLength"] as? Int, n > max { out.append("\(at) maxLength") }
        switch schema["format"] as? String {
        case "base64": if Base64.decode(s) == nil { out.append("\(at) format") }
        case "date-time": if RFC3339.parse(s) == nil || !(s.hasSuffix("Z") || s.range(of: #"[+-]\d\d:\d\d$"#, options: .regularExpression) != nil) { out.append("\(at) format") }
        default: break
        }
    }
    return out
}

/// Calls a capability and splits its MCP result; checks the result's shape on the way.
func run(_ capability: Capability, _ tool: String, _ args: Any) -> (structured: [String: Any]?, error: String?) {
    // As the Engine does: from a worker thread (the main thread is refused).
    var raw = ""
    let done = DispatchSemaphore(value: 0)
    let arguments = text(args)
    Thread.detachNewThread { raw = try! capability.call(tool: tool, argumentsJson: arguments); done.signal() }
    done.wait()
    let result = json(raw) as! [String: Any]
    let content = result["content"] as! [[String: Any]]
    check(content.count == 1 && content[0]["type"] as? String == "text", "\(tool): one text content")
    if result["isError"] as? Bool == true {
        check(result["structuredContent"] == nil, "\(tool): an error has no structuredContent")
        return (nil, content[0]["text"] as? String)
    }
    let structured = result["structuredContent"] as! [String: Any]
    check(text(json(content[0]["text"] as! String)) == text(structured), "\(tool): the text is the structuredContent")
    let name = "\(capability.name()).\(tool)"
    let problems = violations(structured, tools[name]!["nativeOutputSchema"] as! [String: Any])
    check(problems.isEmpty, "\(name): off nativeOutputSchema \(problems) in \(text(structured))")
    return (structured, nil)
}

// MARK: fakes

final class FakeLibrary: PhotoLibrary {
    var refs: [PhotoRef] = []
    var files: [String: Data] = [:]
    var allowed = true
    var asked = 0
    func authorize() throws -> Bool { asked += 1; return allowed }
    func count() throws -> Int { refs.count }
    func photos(offset: Int, limit: Int) throws -> [PhotoRef] { Array(refs.dropFirst(offset).prefix(limit)) }
    func photo(id: String) throws -> PhotoFile? {
        guard let ref = refs.first(where: { $0.id == id }) else { return nil }
        return PhotoFile(ref: ref, data: files[id] ?? Data())
    }
}

final class FakePicker: PhotoPicker {
    var answer: [PhotoFile] = []
    var limits: [Int] = []
    func pick(limit: Int) throws -> [PhotoFile] { limits.append(limit); return answer }
}

final class FakeDocuments: DocumentPicker {
    var opened: [PickedFile] = []
    var saveAnswer = true
    var types: [UTType] = []
    var multiple: Bool?
    var saved: [PickedFile] = []
    func open(types: [UTType], multiple: Bool) throws -> [PickedFile] { self.types = types; self.multiple = multiple; return opened }
    func save(_ file: PickedFile) throws -> Bool { saved.append(file); return saveAnswer }
}

final class FakeShare: ShareSheet {
    var answer = true
    var items: [ShareItems] = []
    func share(_ items: ShareItems) throws -> Bool { self.items.append(items); return answer }
}

final class FakePrint: PrintDialog {
    var answer = true
    var files: [PickedFile] = []
    var failure: CapabilityFailure?
    func print(_ file: PickedFile) throws -> Bool { files.append(file); if let failure { throw failure }; return answer }
}

final class FakeScheduler: NotificationScheduler {
    var status = NotificationAuthorization.granted
    var grant = true
    var requests = 0
    var pending: [String: PlannedNotification] = [:]
    var delivered: Set<String> = []
    var log: [String] = []
    func authorization() throws -> NotificationAuthorization { status }
    func requestAuthorization() throws -> Bool { requests += 1; status = grant ? .granted : .denied; return grant }
    func schedule(_ n: PlannedNotification) throws { log.append("add \(n.id)"); pending[n.id] = n }
    func pendingIds() throws -> [String] { Array(pending.keys) }
    func deliveredIds() throws -> [String] { Array(delivered) }
    func remove(ids: [String]) throws { log.append("remove \(ids)"); for id in ids { pending[id] = nil; delivered.remove(id) } }
}

let utc: Calendar = { var c = Calendar(identifier: .gregorian); c.timeZone = TimeZone(identifier: "UTC")!; return c }()
let now = date("2026-11-06T10:00:00Z")
func photoRef(_ o: [String: Any]) -> PhotoRef {
    PhotoRef(id: o["id"] as! String, name: o["name"] as! String, mimeType: o["mimeType"] as! String, takenAt: (o["takenAt"] as? String).map(date))
}

// MARK: the tools

let library = FakeLibrary(), picker = FakePicker(), documents = FakeDocuments(), share = FakeShare(), printer = FakePrint(), scheduler = FakeScheduler()
let photos = PhotosCapability(library: library, picker: picker)
let files = FilesCapability(picker: documents)
let sharing = ShareCapability(sheet: share)
let printing = PrintCapability(dialog: printer)
let notifications = NotificationsCapability(scheduler: scheduler, now: { now }, calendar: utc)
let all: [Capability] = [photos, files, sharing, printing, notifications]

for capability in all {
    let served = (json(capability.toolsJson()) as! [[String: Any]]).map { "\(capability.name()).\($0["name"] as! String)" }.sorted()
    let native = tools.values.filter { ($0["name"] as! String).hasPrefix("\(capability.name()).") && $0["runs"] as? String == "native" }.map { $0["name"] as! String }.sorted()
    check(served == native, "\(capability.name()) serves every native tool: \(served) vs \(native)")
}
check(Set(all.map { $0.name() }) == Set(tools.keys.map { String($0.split(separator: ".")[0]) }), "one capability per catalog name")

// MARK: bridgeCases: each native call answers its nativeResult.

var replayed = 0
for c in fixture["bridgeCases"] as! [[String: Any]] {
    guard let native = c["native"] as? [String: Any], let expected = c["nativeResult"] as? [String: Any] else { continue }
    let name = c["name"] as! String, toolName = c["tool"] as! String
    let schema = tools[toolName]!["nativeOutputSchema"] as! [String: Any]
    let wanted = expected["structuredContent"] as? [String: Any]
    if expected["isError"] == nil && (wanted == nil || !violations(wanted!, schema).isEmpty) { continue } // the bridge's own refusals
    let parts = toolName.split(separator: ".").map(String.init)
    let capability = all.first { $0.name() == parts[0] }!
    switch toolName {
    case "photos.pick":
        picker.answer = (wanted!["photos"] as! [[String: Any]]).map { PhotoFile(ref: photoRef($0), data: Base64.decode($0["data"] as! String)!) }
    case "photos.list":
        library.refs = (wanted!["photos"] as! [[String: Any]]).map(photoRef) + [PhotoRef(id: "more", name: "x.jpg", mimeType: "image/jpeg", takenAt: nil)]
    case "photos.get":
        library.refs = []
        if let photo = wanted?["photo"] as? [String: Any] { library.refs = [photoRef(photo)]; library.files[photo["id"] as! String] = Base64.decode(photo["data"] as! String) }
    case "files.open":
        documents.opened = (wanted!["files"] as! [[String: Any]]).map { PickedFile(name: $0["name"] as! String, mimeType: $0["mimeType"] as? String, data: Base64.decode($0["data"] as! String)!) }
    case "files.save": documents.saveAnswer = wanted!["saved"] as! Bool
    case "share.send": share.answer = wanted!["shared"] as! Bool
    case "print.print": printer.answer = wanted!["printed"] as! Bool
    case "notifications.cancel":
        if wanted!["cancelled"] as! Bool { scheduler.delivered.insert(native["id"] as! String) }
    default: break
    }
    let (got, error) = run(capability, parts[1], native)
    replayed += 1
    if let reason = expected["isError"] as? String {
        check(error == reason, "\(name): error \(error ?? "none")")
        continue
    }
    guard let got else { check(false, "\(name): failed \(error ?? "")"); continue }
    if toolName == "photos.list" {
        check(text(got["photos"]!) == text(wanted!["photos"]!), "\(name): \(text(got))")
        check(got["cursor"] as? String == PhotoCursor.encode(offset: 1), "\(name): the cursor \(text(got))")
    } else {
        check(text(got) == text(wanted!), "\(name): \(text(got)) vs \(text(wanted!))")
    }
    // The native side received the bridge's arguments.
    switch toolName {
    case "photos.pick": check(picker.limits.last == native["limit"] as? Int, "\(name): the picker's limit")
    case "files.open": check(documents.multiple == native["multiple"] as? Bool && documents.types == [.item], "\(name): open \(documents.types)")
    case "files.save", "print.print":
        let file = toolName == "files.save" ? documents.saved.last! : printer.files.last!
        check(file == PickedFile(name: native["name"] as! String, mimeType: native["mimeType"] as? String, data: Base64.decode(native["data"] as! String)!), "\(name): the file \(file)")
    case "share.send":
        let items = share.items.last!
        let f = native["file"] as? [String: Any]
        check(items.text == native["text"] as? String && items.url?.absoluteString == native["url"] as? String && items.title == native["title"] as? String
              && items.file == f.map { PickedFile(name: $0["name"] as! String, mimeType: $0["mimeType"] as? String, data: Base64.decode($0["data"] as! String)!) },
              "\(name): the items \(items)")
    case "notifications.post":
        let id = native["id"] as! String
        let planned = scheduler.pending[id]!
        check(planned.title == native["title"] as? String && planned.body == native["body"] as? String, "\(name): the content")
        if let at = native["at"] as? String {
            let components = DateComponents(timeZone: utc.timeZone, year: 2026, month: 11, day: 7, hour: 9, minute: 0, second: 0)
            check(at == "2026-11-07T09:00:00Z" && planned.trigger == .on(components), "\(name): scheduled \(planned.trigger)")
        } else { check(planned.trigger == .now, "\(name): now") }
        let tap = NotificationTaps.tapJson(userInfo: planned.userInfo)
        if let wantedTap = native["tap"] as? [String: Any] {
            check(tap.map { text(json($0)) } == text(wantedTap), "\(name): the tap round trip \(tap ?? "nil")")
        } else { check(tap == nil, "\(name): no tap") }
        check(NotificationTaps.engineId(userInfo: planned.userInfo) == id, "\(name): the id rides along")
    case "notifications.cancel":
        check(scheduler.log.last == "remove [\"\(native["id"] as! String)\"]", "\(name): removed pending + delivered")
    default: break
    }
}
check(replayed >= 15, "replayed \(replayed) bridge cases")

// MARK: semantics

// Cancelled pickers and dialogs are not errors.
picker.answer = []
check(text(run(photos, "pick", ["limit": 3]).structured!) == #"{"photos":[]}"#, "a cancelled photo picker")
documents.opened = []
check(text(run(files, "open", ["multiple": false]).structured!) == #"{"files":[]}"#, "a cancelled open dialog")
documents.saveAnswer = false
check(text(run(files, "save", ["name": "a.txt", "mimeType": "text/plain", "data": ""]).structured!) == #"{"saved":false}"#, "a cancelled save")
share.answer = false
check(text(run(sharing, "send", ["text": "x"]).structured!) == #"{"shared":false}"#, "a dismissed share sheet")
printer.answer = false
check(text(run(printing, "print", ["name": "a.pdf", "mimeType": "application/pdf", "data": "JVBERi0xLjQK"]).structured!) == #"{"printed":false}"#, "a cancelled print")
printer.failure = .failed("this file cannot be printed")
check(run(printing, "print", ["name": "a.pdf", "mimeType": "application/pdf", "data": "JVBERi0xLjQK"]).error == "failed: this file cannot be printed", "a print failure")
printer.failure = nil

// The picker never answers more than asked; one file unless multiple.
picker.answer = (1...4).map { PhotoFile(ref: PhotoRef(id: "p\($0)", name: "IMG_\($0).HEIC", mimeType: "image/heic", takenAt: nil), data: Data([UInt8($0)])) }
let picked = run(photos, "pick", ["limit": 2]).structured!["photos"] as! [[String: Any]]
check(picked.count == 2 && picked[0]["mimeType"] as? String == "image/heic" && picked[0]["data"] as? String == "AQ==" && picked[0]["takenAt"] is NSNull, "pick ≤ limit, HEIC as image/heic: \(picked)")
documents.opened = [PickedFile(name: "a.csv", mimeType: nil, data: Data("a".utf8)), PickedFile(name: "b.heic", mimeType: nil, data: Data())]
let one = run(files, "open", ["accept": ["image/*", ".csv", "application/pdf"], "multiple": false]).structured!["files"] as! [[String: Any]]
check(one.count == 1 && one[0]["mimeType"] as? String == "text/csv", "one file without multiple, its type by extension: \(one)")
check(documents.types == [.image, UTType(filenameExtension: "csv")!, .pdf], "accept → content types: \(documents.types)")
let both = run(files, "open", ["multiple": true]).structured!["files"] as! [[String: Any]]
check(both.count == 2 && both[1]["mimeType"] as? String == "image/heic" && both[1]["data"] as? String == "", "multiple, empty bytes: \(both)")

// The cursor pages the library newest first and ends with null.
library.refs = (0..<5).map { PhotoRef(id: "ph-\($0)", name: "IMG_\($0).jpg", mimeType: "image/jpeg", takenAt: date("2026-11-0\($0 + 1)T10:00:00+01:00")) }
var cursor: Any = NSNull()
var seen: [String] = []
for _ in 0..<5 {
    var args: [String: Any] = ["limit": 2]
    if let c = cursor as? String { args["cursor"] = c }
    let page = run(photos, "list", args).structured!
    seen += (page["photos"] as! [[String: Any]]).map { $0["id"] as! String }
    cursor = page["cursor"]!
    if cursor is NSNull { break }
}
check(seen == ["ph-0", "ph-1", "ph-2", "ph-3", "ph-4"] && cursor is NSNull, "the cursor pages: \(seen)")
let first = run(photos, "list", ["limit": 1]).structured!["photos"] as! [[String: Any]]
check(first[0]["takenAt"] as? String == "2026-11-01T09:00:00Z", "takenAt in UTC: \(first)")
check(run(photos, "list", ["limit": 2, "cursor": "o99"]).structured.map(text) == #"{"cursor":null,"photos":[]}"#, "past the end")
check(run(photos, "list", ["limit": 2, "cursor": "nope"]).error == "failed: bad cursor", "a foreign cursor")
check(run(photos, "list", ["limit": 2, "cursor": "o01"]).error == "failed: bad cursor", "a non-canonical cursor")
library.refs = []
check(text(run(photos, "list", ["limit": 50]).structured!) == #"{"cursor":null,"photos":[]}"#, "an empty library")
check(run(photos, "get", ["id": "gone"]).error == "notFound", "get: an unknown id")
library.allowed = false
check(run(photos, "list", ["limit": 1]).error == "failed: photo library access denied", "list: access denied")
check(run(photos, "get", ["id": "x"]).error == "failed: photo library access denied", "get: access denied")
library.allowed = true

// Notifications: authorization on first post, replace, cancel, past = now.
let fresh = FakeScheduler()
fresh.status = .notDetermined
let n2 = NotificationsCapability(scheduler: fresh, now: { now }, calendar: utc)
_ = run(n2, "post", ["id": "a/1", "title": "t", "body": NSNull(), "at": NSNull(), "tap": NSNull()])
_ = run(n2, "post", ["id": "a/1", "title": "t2", "body": "b", "at": "2026-11-06T09:00:00Z", "tap": NSNull()])
check(fresh.requests == 1, "authorization is requested once: \(fresh.requests)")
check(fresh.pending.count == 1 && fresh.pending["a/1"]?.title == "t2" && fresh.pending["a/1"]?.trigger == .now, "the same id replaces; the past = now")
check(fresh.log == ["remove [\"a/1\"]", "add a/1", "remove [\"a/1\"]", "add a/1"], "replace withdraws the shown one first: \(fresh.log)")
_ = run(n2, "post", ["id": "a/2", "title": "soon", "body": NSNull(), "at": "2026-11-06T10:00:30.500Z", "tap": NSNull()])
check(fresh.pending["a/2"]?.trigger == .after(30.5), "under a minute = an interval: \(String(describing: fresh.pending["a/2"]?.trigger))")
check(text(run(n2, "cancel", ["id": "a/1"]).structured!) == #"{"cancelled":true}"#, "cancel a pending one")
check(text(run(n2, "cancel", ["id": "a/1"]).structured!) == #"{"cancelled":false}"#, "cancel again: none")
let denied = FakeScheduler()
denied.status = .notDetermined
denied.grant = false
let n3 = NotificationsCapability(scheduler: denied, now: { now })
check(run(n3, "post", ["id": "a/1", "title": "t", "body": NSNull(), "at": NSNull(), "tap": NSNull()]).error == "failed: notifications not allowed", "denied")
check(denied.pending.isEmpty, "nothing scheduled when denied")
check(NotificationTaps.tapJson(userInfo: [:]) == nil && NotificationTaps.tapJson(userInfo: ["other": 1]) == nil, "a foreign notification has no tap")
let tap = #"{"app":"acme.album","params":{"id":"p2","n":1.5},"screen":"page","type":"notificationTap"}"#
_ = run(notifications, "post", ["id": "acme.album/x", "title": "t", "body": NSNull(), "at": NSNull(), "tap": json(tap)])
check(NotificationTaps.tapJson(userInfo: scheduler.pending["acme.album/x"]!.userInfo as [AnyHashable: Any]) == tap, "the tap JSON round trips verbatim")

// Unavailable without a seam; refused on the main thread; unknown tools.
for capability in [PhotosCapability(library: nil, picker: nil), FilesCapability(picker: nil), ShareCapability(sheet: nil), PrintCapability(dialog: nil), NotificationsCapability(scheduler: nil)] as [Capability] {
    for tool in (json(capability.toolsJson()) as! [[String: Any]]).map({ $0["name"] as! String }) {
        let args: [String: Any] = ["limit": 1, "id": "a/1", "title": "t", "multiple": false, "name": "a", "mimeType": "text/plain", "data": "", "text": "x"]
        check(run(capability, tool, args).error == "unavailable", "\(capability.name()).\(tool) without a seam")
    }
}
let calls = (share.items.count, picker.limits.count, scheduler.log.count)
for capability in all { // called right here, on the main thread
    for tool in (json(capability.toolsJson()) as! [[String: Any]]).map({ $0["name"] as! String }) {
        let result = json(try! capability.call(tool: tool, argumentsJson: #"{"text":"x","limit":1,"id":"a/1","title":"t"}"#)) as! [String: Any]
        check(result["isError"] as? Bool == true && ((result["content"] as! [[String: Any]])[0]["text"] as? String) == "failed: main thread",
              "\(capability.name()).\(tool) refuses the main thread")
    }
}
check((share.items.count, picker.limits.count, scheduler.log.count) == calls, "the main thread never reaches a seam")
check(run(photos, "zoom", [:]).error == "failed: unknown tool photos.zoom", "an unknown tool")
check(run(photos, "pick", ["limit": "two"]).error?.hasPrefix("failed: arguments") == true, "bad arguments")
check(run(sharing, "send", ["title": "only"]).error == "failed: nothing to share", "nothing to share")

// The pure helpers.
check(Base64.encode(Data("hello".utf8)) == "aGVsbG8=" && Base64.decode("a b") == nil && Base64.decode("aGk") == nil, "standard padded base64")
check(FileNames.clamp("a/b/IMG.HEIC", fallback: "f") == "IMG.HEIC" && FileNames.clamp("  ", fallback: "f") == "f" && FileNames.clamp(String(repeating: "é", count: 300), fallback: "f").unicodeScalars.count == 255, "names clamp")
check(FileNames.safe("a:b/c") == "a-b-c", "safe temp names")
check(FileNames.withExtension("IMG_1.HEIC", "jpeg") == "IMG_1.jpeg" && FileNames.withExtension("IMG_1.HEIC", "heic") == "IMG_1.HEIC" && FileNames.withExtension("photo", "png") == "photo.png", "extensions follow the bytes")
check(MimeTypes.forTypeIdentifier("public.heic") == "image/heic" && MimeTypes.forTypeIdentifier("public.jpeg") == "image/jpeg" && MimeTypes.forName("x.unknownext") == "application/octet-stream", "MIME types")
check(AcceptedTypes.contentTypes(nil) == [.item] && AcceptedTypes.contentTypes(["*/*"]) == [.item] && AcceptedTypes.contentTypes(["video/*", "VIDEO/*"]) == [.movie], "accept wildcards")
check(NotificationPlan.trigger(at: nil, now: now) == .now && NotificationPlan.trigger(at: now, now: now) == .now, "now")
check(RFC3339.format(date("2026-11-07T10:00:00.250+01:00")) == "2026-11-07T09:00:00Z", "RFC 3339 in UTC")
do { _ = try PhotoCursor.decode("o-1"); check(false, "a negative cursor") } catch { check(true, "") }

// Blocking: the worker waits for the main queue; the main thread is refused.
do { let _: Int = try Blocking.onMain { $0(.success(1)) }; check(false, "onMain on the main thread") }
catch { check((error as? CapabilityFailure) == .mainThread, "onMain refuses the main thread") }
var hopped: (Bool, Int)?
Thread.detachNewThread {
    let value: Int? = try? Blocking.onMain { finish in finish(.success(Thread.isMainThread ? 7 : 0)); finish(.success(8)) }
    let async: Int? = try? Blocking.wait { finish in DispatchQueue.global().async { finish(.success(9)) } }
    DispatchQueue.main.async { hopped = (value == 7, async ?? 0) }
}
let deadline = Date().addingTimeInterval(10)
while hopped == nil && Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.01)) }
check(hopped?.0 == true && hopped?.1 == 9, "a worker hops to the main queue and waits (first answer wins): \(String(describing: hopped))")

print("\(checks - failures)/\(checks) checks passed (capabilities, \(replayed) bridge cases replayed)")
exit(failures == 0 ? 0 : 1)
