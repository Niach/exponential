// #VAPP-265 (Engine E5), #VAPP-268 (one module per capability): the shared
// pure half of the Swift capabilities. Arguments
// in, MCP call results out (packages/eengine/fixtures/capabilities.json,
// `tools[].nativeOutputSchema`), base64, RFC 3339, names and MIME types, and
// the one threading rule: a call blocks its caller and REFUSES the main thread.
import Foundation
import UniformTypeIdentifiers
import ExponentialEngine

/// A refusal a seam (or a capability) answers as an MCP error result: the
/// reason is the result's text (`notFound`, `unavailable`, `failed: …`).
public struct CapabilityFailure: Error, Equatable {
    public let reason: String
    public init(_ reason: String) { self.reason = reason }

    public static let notFound = CapabilityFailure("notFound")
    public static let unavailable = CapabilityFailure("unavailable")
    public static let mainThread = CapabilityFailure("failed: main thread")
    public static func failed(_ message: String) -> CapabilityFailure {
        CapabilityFailure("failed: \(String(message.prefix(200)))")
    }
}

/// MCP call results: `structuredContent` + the same JSON as its text, or an error.
package enum MCP {
    package static func ok(_ structured: [String: Any]) -> String {
        let body = JSON.text(structured)
        return JSON.text(["content": [["type": "text", "text": body]], "structuredContent": structured])
    }

    package static func error(_ reason: String) -> String {
        JSON.text(["content": [["type": "text", "text": reason]], "isError": true])
    }
}

package enum JSON {
    package static func text(_ value: Any) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .fragmentsAllowed, .withoutEscapingSlashes]) else { return "null" }
        return String(decoding: data, as: UTF8.self)
    }

    package static func value(_ text: String) -> Any? {
        try? JSONSerialization.jsonObject(with: Data(text.utf8), options: [.fragmentsAllowed])
    }
}

/// The bridge's native arguments (already validated by the Engine); a
/// mismatch here is a host bug and answers `failed: arguments …`.
package struct Arguments {
    package let values: [String: Any]

    package init(json: String) throws {
        guard let object = JSON.value(json) as? [String: Any] else { throw CapabilityFailure.failed("arguments are not a JSON object") }
        values = object
    }

    private func present(_ key: String) -> Any? {
        guard let value = values[key], !(value is NSNull) else { return nil }
        return value
    }

    private func wrong(_ key: String, _ type: String) -> CapabilityFailure { .failed("arguments: \(key) is not \(type)") }

    package func string(_ key: String) throws -> String {
        guard let value = try optionalString(key) else { throw CapabilityFailure.failed("arguments: \(key) is missing") }
        return value
    }

    package func optionalString(_ key: String) throws -> String? {
        guard let value = present(key) else { return nil }
        guard let text = value as? String else { throw wrong(key, "a string") }
        return text
    }

    package func int(_ key: String, default fallback: Int, in range: ClosedRange<Int>) throws -> Int {
        guard let value = present(key) else { return fallback }
        guard let number = value as? NSNumber, !number.isBool, number.doubleValue == number.doubleValue.rounded(),
              range.contains(number.intValue) else { throw wrong(key, "an integer in \(range)") }
        return number.intValue
    }

    package func bool(_ key: String, default fallback: Bool) throws -> Bool {
        guard let value = present(key) else { return fallback }
        guard let number = value as? NSNumber, number.isBool else { throw wrong(key, "a boolean") }
        return number.boolValue
    }

    package func strings(_ key: String) throws -> [String]? {
        guard let value = present(key) else { return nil }
        guard let list = value as? [String] else { throw wrong(key, "a list of strings") }
        return list
    }

    package func object(_ key: String) throws -> [String: Any]? {
        guard let value = present(key) else { return nil }
        guard let object = value as? [String: Any] else { throw wrong(key, "an object") }
        return object
    }

    /// `{name, mimeType, data}` (bytes as standard padded base64).
    package func file(_ key: String) throws -> PickedFile? {
        guard let object = try object(key) else { return nil }
        let inner = Arguments(values: object)
        return PickedFile(name: try inner.string("name"), mimeType: try inner.string("mimeType"), data: try inner.bytes("data"))
    }

    package func bytes(_ key: String) throws -> Data {
        guard let data = Base64.decode(try string(key)) else { throw wrong(key, "base64") }
        return data
    }

    private init(values: [String: Any]) { self.values = values }
}

extension NSNumber {
    package var isBool: Bool { CFGetTypeID(self) == CFBooleanGetTypeID() }
}

public enum Base64 {
    /// The standard alphabet, padded (what the bridge's `format: base64` accepts).
    public static func encode(_ data: Data) -> String { data.base64EncodedString() }
    public static func decode(_ text: String) -> Data? { Data(base64Encoded: text) }
}

public enum RFC3339 {
    /// `2026-11-01T10:00:00Z` (UTC, whole seconds).
    public static func format(_ date: Date) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        formatter.timeZone = TimeZone(identifier: "UTC")
        return formatter.string(from: date)
    }

    /// RFC 3339 with a zone, fractional seconds optional.
    public static func parse(_ text: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: text) { return date }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: text)
    }
}

public enum FileNames {
    /// The last path component, trimmed, at most 255 code points; `fallback` when empty.
    public static func clamp(_ name: String?, fallback: String) -> String {
        var base = (name ?? "").split(separator: "/").last.map(String.init) ?? ""
        base = base.trimmingCharacters(in: .whitespacesAndNewlines)
        if base.isEmpty || base == "." || base == ".." { base = fallback }
        return String(String.UnicodeScalarView(base.unicodeScalars.prefix(255)))
    }

    /// A name safe as a temp file's last component (save, share).
    public static func safe(_ name: String) -> String {
        let cleaned = String(name.map { $0 == "/" || $0 == ":" || $0 == "\\" ? "-" : $0 })
        return clamp(cleaned, fallback: "file")
    }

    /// `name` with the extension `ext` (replaces a different one, keeps a matching one).
    public static func withExtension(_ name: String, _ ext: String?) -> String {
        guard let ext, !ext.isEmpty else { return name }
        let url = URL(fileURLWithPath: name)
        if url.pathExtension.lowercased() == ext.lowercased() { return name }
        let stem = url.pathExtension.isEmpty ? name : url.deletingPathExtension().lastPathComponent
        return clamp("\(stem).\(ext)", fallback: "file.\(ext)")
    }
}

public enum MimeTypes {
    package static let octetStream = "application/octet-stream"

    /// A type identifier's MIME type (`public.heic` → `image/heic`).
    public static func forTypeIdentifier(_ identifier: String?) -> String? {
        guard let identifier, let type = UTType(identifier) else { return nil }
        return clip(type.preferredMIMEType ?? known(type))
    }

    /// A file name's MIME type by its extension, else `application/octet-stream`.
    public static func forName(_ name: String) -> String {
        let ext = URL(fileURLWithPath: name).pathExtension
        guard !ext.isEmpty, let type = UTType(filenameExtension: ext) else { return octetStream }
        return clip(type.preferredMIMEType ?? known(type)) ?? octetStream
    }

    private static func known(_ type: UTType) -> String? {
        if type.conforms(to: .heic) { return "image/heic" }
        if type.conforms(to: .heif) { return "image/heif" }
        return nil
    }

    private static func clip(_ mime: String?) -> String? {
        guard let mime, !mime.isEmpty else { return nil }
        return String(mime.prefix(100))
    }
}

/// files.open's `accept` (MIME types, `image/*` wildcards, `.ext`) as content types.
public enum AcceptedTypes {
    public static func contentTypes(_ accept: [String]?) -> [UTType] {
        var types: [UTType] = []
        for entry in accept ?? [] {
            let item = entry.trimmingCharacters(in: .whitespaces).lowercased()
            let type: UTType?
            if item.hasPrefix(".") {
                type = item.count > 1 ? UTType(filenameExtension: String(item.dropFirst())) : nil
            } else if item == "*/*" || item == "*" {
                type = .item
            } else if item.hasSuffix("/*") {
                switch item.dropLast(2) {
                case "image": type = .image
                case "video": type = .movie
                case "audio": type = .audio
                case "text": type = .text
                case "font": type = .font
                case "application": type = .data
                default: type = nil
                }
            } else {
                type = item.contains("/") ? UTType(mimeType: item) : nil
            }
            if let type, !types.contains(type) { types.append(type) }
        }
        return types.isEmpty ? [.item] : types
    }
}

/// The blocking call's threading: the Engine calls a capability on a worker
/// thread; UI work hops to the main queue and the worker waits for its answer.
public enum Blocking {
    /// Runs `body` on the main queue and waits until it calls `finish` (once;
    /// later calls are ignored). Refuses the main thread (it would deadlock).
    public static func onMain<T>(_ body: @escaping @MainActor (_ finish: @escaping (Result<T, Error>) -> Void) -> Void) throws -> T {
        try wait { finish in
            DispatchQueue.main.async { MainActor.assumeIsolated { body(finish) } }
        }
    }

    /// Starts `body` (an async platform API) and waits until it calls `finish`.
    public static func wait<T>(_ body: (_ finish: @escaping (Result<T, Error>) -> Void) -> Void) throws -> T {
        if Thread.isMainThread { throw CapabilityFailure.mainThread }
        let box = Once<T>()
        body { box.set($0) }
        return try box.wait().get()
    }

    final class Once<T>: @unchecked Sendable {
        private let lock = NSLock()
        private let semaphore = DispatchSemaphore(value: 0)
        private var result: Result<T, Error>?

        func set(_ value: Result<T, Error>) {
            lock.lock()
            defer { lock.unlock() }
            guard result == nil else { return }
            result = value
            semaphore.signal()
        }

        func wait() -> Result<T, Error> {
            semaphore.wait()
            lock.lock()
            defer { lock.unlock() }
            return result!
        }
    }
}

/// What every capability shares: the main-thread refusal, argument decoding
/// and the mapping of thrown failures to error results.
package struct Runner {
    package let capability: String
    package let isMainThread: () -> Bool

    package init(capability: String, isMainThread: @escaping () -> Bool) {
        self.capability = capability
        self.isMainThread = isMainThread
    }

    package func run(_ tool: String, _ argumentsJson: String, _ body: (String, Arguments) throws -> [String: Any]) -> String {
        if isMainThread() { return MCP.error(CapabilityFailure.mainThread.reason) }
        do {
            return MCP.ok(try body(tool, try Arguments(json: argumentsJson)))
        } catch let failure as CapabilityFailure {
            return MCP.error(failure.reason)
        } catch {
            return MCP.error(CapabilityFailure.failed(error.localizedDescription).reason)
        }
    }

    package func unknown(_ tool: String) -> CapabilityFailure { .failed("unknown tool \(capability).\(tool)") }
}

/// The native tools each capability serves (bare names; the Engine's catalog
/// owns the app-facing schemas, these describe the NATIVE arguments).
package enum ToolCatalog {
    private static let string: [String: Any] = ["type": "string"]
    private static let base64: [String: Any] = ["type": "string", "format": "base64"]
    private static let nullableString: [String: Any] = ["type": ["string", "null"]]
    private static let fileObject: [String: Any] = [
        "type": "object",
        "properties": ["name": string, "mimeType": string, "data": base64],
        "required": ["name", "mimeType", "data"],
        "additionalProperties": false,
    ]

    private static func tool(_ name: String, _ description: String, _ properties: [String: Any], required: [String] = []) -> [String: Any] {
        ["name": name, "description": description,
         "inputSchema": ["type": "object", "properties": properties, "required": required, "additionalProperties": false]]
    }

    package static func json(_ capability: String) -> String {
        let tools: [[String: Any]]
        switch capability {
        case "photos":
            tools = [
                tool("get", "One photo of the library by its id, with its bytes.", ["id": string], required: ["id"]),
                tool("list", "The library's photos, newest first, without bytes; cursor null = the end.",
                     ["limit": ["type": "integer", "minimum": 1, "maximum": 100], "cursor": string], required: ["limit"]),
                tool("pick", "The platform's photo picker; an empty list when cancelled.",
                     ["limit": ["type": "integer", "minimum": 1, "maximum": 50]], required: ["limit"]),
            ]
        case "files":
            tools = [
                tool("open", "The platform's open dialog; an empty list when cancelled.",
                     ["accept": ["type": "array", "items": string], "multiple": ["type": "boolean"]], required: ["multiple"]),
                tool("save", "The platform's save dialog; saved = false when cancelled.",
                     ["name": string, "mimeType": string, "data": base64], required: ["name", "mimeType", "data"]),
            ]
        case "share":
            tools = [tool("send", "The platform's share sheet; shared = false when dismissed.",
                          ["title": string, "text": string, "url": string, "file": fileObject])]
        case "print":
            tools = [tool("print", "The platform's print dialog for a PDF, PNG or JPEG; printed = false when cancelled.",
                          ["name": string, "mimeType": string, "data": base64], required: ["name", "mimeType", "data"])]
        case "notifications":
            tools = [
                tool("cancel", "Withdraw a pending or shown notification; cancelled = whether one existed.", ["id": string], required: ["id"]),
                tool("post", "Post now (at null or past) or schedule; the tap comes back through NotificationTaps.",
                     ["id": string, "title": string, "body": nullableString, "at": nullableString, "tap": ["type": ["object", "null"]]],
                     required: ["id", "title"]),
            ]
        default:
            tools = []
        }
        return JSON.text(tools)
    }
}

/// A file the person opened, or one handed to save/share/print.
public struct PickedFile: Equatable {
    public var name: String
    public var mimeType: String?
    public var data: Data
    public init(name: String, mimeType: String?, data: Data) { self.name = name; self.mimeType = mimeType; self.data = data }
}
