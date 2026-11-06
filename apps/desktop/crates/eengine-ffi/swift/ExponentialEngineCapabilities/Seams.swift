// #VAPP-265: the platform seams. Each capability talks to the platform only
// through these protocols, so the pure parts run against fakes on any host;
// `Platform+iOS.swift` and `Notifications+UN.swift` are the real ones.
//
// Every seam method BLOCKS its caller (a worker thread) until the person or
// the platform answers, and throws `CapabilityFailure` (or any error, which
// answers `failed: <its description>`).
import Foundation
import UniformTypeIdentifiers

/// A library photo without its bytes.
public struct PhotoRef: Equatable {
    public var id: String
    public var name: String
    public var mimeType: String
    public var takenAt: Date?
    public init(id: String, name: String, mimeType: String, takenAt: Date?) {
        self.id = id; self.name = name; self.mimeType = mimeType; self.takenAt = takenAt
    }
}

/// A photo with its bytes (`get`, `pick`).
public struct PhotoFile: Equatable {
    public var ref: PhotoRef
    public var data: Data
    public init(ref: PhotoRef, data: Data) { self.ref = ref; self.data = data }
}

/// A file the person opened, or one handed to save/share/print.
public struct PickedFile: Equatable {
    public var name: String
    public var mimeType: String?
    public var data: Data
    public init(name: String, mimeType: String?, data: Data) { self.name = name; self.mimeType = mimeType; self.data = data }
}

public protocol PhotoLibrary: AnyObject {
    /// Whether the library may be read; requests access when not decided yet.
    func authorize() throws -> Bool
    /// The number of photos.
    func count() throws -> Int
    /// Photos `offset ..< offset + limit`, newest first.
    func photos(offset: Int, limit: Int) throws -> [PhotoRef]
    /// One photo with its bytes; nil = no such photo.
    func photo(id: String) throws -> PhotoFile?
}

public protocol PhotoPicker: AnyObject {
    /// The platform's picker (≤ `limit`); [] = cancelled.
    func pick(limit: Int) throws -> [PhotoFile]
}

public protocol DocumentPicker: AnyObject {
    /// The platform's open dialog for `types`; [] = cancelled.
    func open(types: [UTType], multiple: Bool) throws -> [PickedFile]
    /// The platform's save (export) dialog; false = cancelled.
    func save(_ file: PickedFile) throws -> Bool
}

/// What share.send hands the sheet.
public struct ShareItems: Equatable {
    public var title: String?
    public var text: String?
    public var url: URL?
    public var file: PickedFile?
    public init(title: String?, text: String?, url: URL?, file: PickedFile?) {
        self.title = title; self.text = text; self.url = url; self.file = file
    }
}

public protocol ShareSheet: AnyObject {
    /// The platform's share sheet; false = dismissed.
    func share(_ items: ShareItems) throws -> Bool
}

public protocol PrintDialog: AnyObject {
    /// The platform's print dialog for a PDF, PNG or JPEG; false = cancelled.
    func print(_ file: PickedFile) throws -> Bool
}

public enum NotificationAuthorization: Equatable { case notDetermined, denied, granted }

/// When a notification fires.
public enum NotificationTrigger: Equatable {
    /// Deliver now (`at` null or not in the future).
    case now
    /// In `seconds` (`at` under a minute ahead).
    case after(TimeInterval)
    /// At these calendar components (`at` further ahead; year … second + the zone).
    case on(DateComponents)
}

/// One notification as the scheduler adds it.
public struct PlannedNotification: Equatable {
    /// The Engine's id (`<app>/<id>`) = the platform request's identifier.
    public var id: String
    public var title: String
    public var body: String?
    public var trigger: NotificationTrigger
    /// `NotificationTaps.userInfo`: the id, and the tap JSON when there is one.
    public var userInfo: [String: String]
    public init(id: String, title: String, body: String?, trigger: NotificationTrigger, userInfo: [String: String]) {
        self.id = id; self.title = title; self.body = body; self.trigger = trigger; self.userInfo = userInfo
    }
}

public protocol NotificationScheduler: AnyObject {
    func authorization() throws -> NotificationAuthorization
    /// Asks the person; true = allowed.
    func requestAuthorization() throws -> Bool
    /// Adds (or, for a known identifier, replaces) the request.
    func schedule(_ notification: PlannedNotification) throws
    func pendingIds() throws -> [String]
    func deliveredIds() throws -> [String]
    /// Withdraws pending AND delivered ones with these identifiers.
    func remove(ids: [String]) throws
}
