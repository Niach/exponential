// #VAPP-265: the five native capabilities (`Capability` of the Engine's
// binding), mounted under their catalog names. Each serves EVERY native tool
// of its capability; without a seam a tool answers `unavailable`. Results
// match capabilities.json's `nativeOutputSchema`; a cancelled picker, dialog
// or sheet is not an error (`{photos: []}`, `{saved: false}`, …).
//
// THREADING: `call` blocks until the person answers. Call it from a worker
// thread (the Engine does); on the main thread it answers `failed: main thread`.
import Foundation
import ExponentialEngine

func encode(_ ref: PhotoRef) -> [String: Any] {
    ["id": String(ref.id.prefix(500)),
     "name": FileNames.clamp(ref.name, fallback: "photo"),
     "mimeType": ref.mimeType.isEmpty ? MimeTypes.forName(ref.name) : String(ref.mimeType.prefix(100)),
     "takenAt": ref.takenAt.map(RFC3339.format) ?? NSNull()]
}

func encode(_ photo: PhotoFile) -> [String: Any] {
    var object = encode(photo.ref)
    object["data"] = Base64.encode(photo.data)
    return object
}

/// photos.list's cursor: an offset token (`o<offset>`), opaque to apps.
public enum PhotoCursor {
    public static func encode(offset: Int) -> String { "o\(offset)" }
    public static func decode(_ cursor: String?) throws -> Int {
        guard let cursor else { return 0 }
        guard cursor.hasPrefix("o"), let offset = Int(cursor.dropFirst()), offset >= 0, String(offset) == cursor.dropFirst() else {
            throw CapabilityFailure.failed("bad cursor")
        }
        return offset
    }
}

/// `photos`: get, list (the library, PhotoKit), pick (the photo picker).
public final class PhotosCapability: Capability, @unchecked Sendable {
    let library: PhotoLibrary?
    let picker: PhotoPicker?
    let runner: Runner

    public init(library: PhotoLibrary?, picker: PhotoPicker?, isMainThread: @escaping () -> Bool = { Thread.isMainThread }) {
        self.library = library
        self.picker = picker
        runner = Runner(capability: "photos", isMainThread: isMainThread)
    }

    public func name() -> String { "photos" }
    public func toolsJson() -> String { ToolCatalog.json("photos") }

    public func call(tool: String, argumentsJson: String) throws -> String {
        runner.run(tool, argumentsJson) { tool, args in
            switch tool {
            case "list":
                guard let library else { throw CapabilityFailure.unavailable }
                let limit = try args.int("limit", default: 50, in: 1...100)
                let offset = try PhotoCursor.decode(try args.optionalString("cursor"))
                guard try library.authorize() else { throw CapabilityFailure.failed("photo library access denied") }
                let total = try library.count()
                let refs = offset < total ? Array(try library.photos(offset: offset, limit: limit).prefix(limit)) : []
                let next = offset + refs.count
                return ["photos": refs.map(encode), "cursor": !refs.isEmpty && next < total ? PhotoCursor.encode(offset: next) : NSNull()]
            case "get":
                guard let library else { throw CapabilityFailure.unavailable }
                let id = try args.string("id")
                guard try library.authorize() else { throw CapabilityFailure.failed("photo library access denied") }
                guard let photo = try library.photo(id: id) else { throw CapabilityFailure.notFound }
                return ["photo": encode(photo)]
            case "pick":
                guard let picker else { throw CapabilityFailure.unavailable }
                let limit = try args.int("limit", default: 1, in: 1...50)
                return ["photos": try picker.pick(limit: limit).prefix(limit).map(encode)]
            default:
                throw runner.unknown(tool)
            }
        }
    }
}

/// `files`: open (the open dialog), save (the save/export dialog).
public final class FilesCapability: Capability, @unchecked Sendable {
    let picker: DocumentPicker?
    let runner: Runner

    public init(picker: DocumentPicker?, isMainThread: @escaping () -> Bool = { Thread.isMainThread }) {
        self.picker = picker
        runner = Runner(capability: "files", isMainThread: isMainThread)
    }

    public func name() -> String { "files" }
    public func toolsJson() -> String { ToolCatalog.json("files") }

    public func call(tool: String, argumentsJson: String) throws -> String {
        runner.run(tool, argumentsJson) { tool, args in
            guard ["open", "save"].contains(tool) else { throw runner.unknown(tool) }
            guard let picker else { throw CapabilityFailure.unavailable }
            if tool == "open" {
                let types = AcceptedTypes.contentTypes(try args.strings("accept"))
                let multiple = try args.bool("multiple", default: false)
                let files = try picker.open(types: types, multiple: multiple).prefix(multiple ? 100 : 1)
                return ["files": files.map { file -> [String: Any] in
                    let name = FileNames.clamp(file.name, fallback: "file")
                    return ["name": name, "mimeType": file.mimeType.map { String($0.prefix(100)) } ?? MimeTypes.forName(name), "data": Base64.encode(file.data)]
                }]
            }
            let file = PickedFile(name: FileNames.safe(try args.string("name")), mimeType: try args.string("mimeType"), data: try args.bytes("data"))
            return ["saved": try picker.save(file)]
        }
    }
}

/// `share`: send (the share sheet).
public final class ShareCapability: Capability, @unchecked Sendable {
    let sheet: ShareSheet?
    let runner: Runner

    public init(sheet: ShareSheet?, isMainThread: @escaping () -> Bool = { Thread.isMainThread }) {
        self.sheet = sheet
        runner = Runner(capability: "share", isMainThread: isMainThread)
    }

    public func name() -> String { "share" }
    public func toolsJson() -> String { ToolCatalog.json("share") }

    public func call(tool: String, argumentsJson: String) throws -> String {
        runner.run(tool, argumentsJson) { tool, args in
            guard tool == "send" else { throw runner.unknown(tool) }
            guard let sheet else { throw CapabilityFailure.unavailable }
            let url = try args.optionalString("url").map { text -> URL in
                guard let url = URL(string: text), url.scheme != nil else { throw CapabilityFailure.failed("arguments: url is not a URL") }
                return url
            }
            var file = try args.file("file")
            if let named = file { file?.name = FileNames.safe(named.name) }
            let items = ShareItems(title: try args.optionalString("title"), text: try args.optionalString("text"), url: url, file: file)
            guard items.text != nil || items.url != nil || items.file != nil else { throw CapabilityFailure.failed("nothing to share") }
            return ["shared": try sheet.share(items)]
        }
    }
}

/// `print`: print (the print dialog; `print.pdf` is the Engine's own).
public final class PrintCapability: Capability, @unchecked Sendable {
    let dialog: PrintDialog?
    let runner: Runner

    public init(dialog: PrintDialog?, isMainThread: @escaping () -> Bool = { Thread.isMainThread }) {
        self.dialog = dialog
        runner = Runner(capability: "print", isMainThread: isMainThread)
    }

    public func name() -> String { "print" }
    public func toolsJson() -> String { ToolCatalog.json("print") }

    public func call(tool: String, argumentsJson: String) throws -> String {
        runner.run(tool, argumentsJson) { tool, args in
            guard tool == "print" else { throw runner.unknown(tool) }
            guard let dialog else { throw CapabilityFailure.unavailable }
            let file = PickedFile(name: FileNames.safe(try args.string("name")), mimeType: try args.string("mimeType"), data: try args.bytes("data"))
            return ["printed": try dialog.print(file)]
        }
    }
}

/// When a notification for `at` fires, seen from `now`.
public enum NotificationPlan {
    public static func trigger(at: Date?, now: Date, calendar: Calendar = .current) -> NotificationTrigger {
        guard let at else { return .now }
        let seconds = at.timeIntervalSince(now)
        if seconds <= 0 { return .now }
        if seconds < 60 { return .after(seconds) }
        var components = calendar.dateComponents([.year, .month, .day, .hour, .minute, .second], from: at)
        components.timeZone = calendar.timeZone
        return .on(components)
    }
}

/// The tap round trip: post stores the tap JSON in the notification's
/// userInfo; the host's `UNUserNotificationCenterDelegate` hands the response
/// back here and sends the JSON VERBATIM to the app instance (`event(tap)`).
public enum NotificationTaps {
    public static let idKey = "at.exponential.engine.id"
    public static let tapKey = "at.exponential.engine.tap"

    public static func userInfo(id: String, tapJson: String?) -> [String: String] {
        var info = [idKey: id]
        if let tapJson { info[tapKey] = tapJson }
        return info
    }

    /// The tap JSON (`{type: notificationTap, app, screen, params}`), nil for a
    /// notification without one or not posted by the Engine.
    public static func tapJson(userInfo: [AnyHashable: Any]) -> String? {
        userInfo[tapKey] as? String
    }

    /// The Engine's id (`<app>/<id>`) of a notification it posted.
    public static func engineId(userInfo: [AnyHashable: Any]) -> String? {
        userInfo[idKey] as? String
    }
}

/// `notifications`: post (now or scheduled; replaces the same id), cancel.
public final class NotificationsCapability: Capability, @unchecked Sendable {
    let scheduler: NotificationScheduler?
    let now: () -> Date
    let calendar: Calendar
    let runner: Runner

    public init(scheduler: NotificationScheduler?, now: @escaping () -> Date = Date.init, calendar: Calendar = .current,
                isMainThread: @escaping () -> Bool = { Thread.isMainThread }) {
        self.scheduler = scheduler
        self.now = now
        self.calendar = calendar
        runner = Runner(capability: "notifications", isMainThread: isMainThread)
    }

    public func name() -> String { "notifications" }
    public func toolsJson() -> String { ToolCatalog.json("notifications") }

    public func call(tool: String, argumentsJson: String) throws -> String {
        runner.run(tool, argumentsJson) { tool, args in
            guard ["post", "cancel"].contains(tool) else { throw runner.unknown(tool) }
            guard let scheduler else { throw CapabilityFailure.unavailable }
            let id = try args.string("id")
            if tool == "cancel" {
                let existed = try scheduler.pendingIds().contains(id) || scheduler.deliveredIds().contains(id)
                try scheduler.remove(ids: [id])
                return ["cancelled": existed]
            }
            let title = try args.string("title")
            let body = try args.optionalString("body")
            let at = try args.optionalString("at").map { text -> Date in
                guard let date = RFC3339.parse(text) else { throw CapabilityFailure.failed("arguments: at is not an RFC 3339 time") }
                return date
            }
            let tap = try args.object("tap").map { JSON.text($0) }
            var allowed: Bool
            switch try scheduler.authorization() {
            case .granted: allowed = true
            case .denied: allowed = false
            case .notDetermined: allowed = try scheduler.requestAuthorization()
            }
            guard allowed else { throw CapabilityFailure.failed("notifications not allowed") }
            try scheduler.remove(ids: [id])
            try scheduler.schedule(PlannedNotification(
                id: id, title: title, body: body,
                trigger: NotificationPlan.trigger(at: at, now: now(), calendar: calendar),
                userInfo: NotificationTaps.userInfo(id: id, tapJson: tap)))
            return [:]
        }
    }
}
