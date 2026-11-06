// #VAPP-265, #VAPP-268: `photos`. Its own module so an iOS app links
// Photos/PhotosUI only when it declares the capability (ITMS-90683).
import Foundation
import ExponentialEngine
import ExponentialEngineCapabilityCore

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
