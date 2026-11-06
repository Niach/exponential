// VAPP-271: the runner's app files (the Engine's BlobStore), handed to the
// Engine so the painter reads the SAME store: an `app-file:<fileId>` image
// src (catalog/host.json media.appFile) is these bytes. On disk it keeps the
// Engine's own FsBlobStore layout under `<dataDir>/blobs` (`<app>/<uuid>` +
// `<app>/<uuid>.json` = {name, mime}), so files written before stay readable;
// without a data dir it lives in memory (tests, previews).
import Foundation
import ExponentialEngine

public final class RunnerBlobs: BlobStore, @unchecked Sendable {
    /// nil = in memory.
    let root: URL?
    private let lock = NSLock()
    private var memory: [String: Data] = [:]

    public init(root: URL?) { self.root = root }

    private func dir(_ app: String) throws -> URL? {
        if app.isEmpty || app.contains("/") || app.contains("\\") || app.hasPrefix(".") {
            throw EngineError.Failure(code: "BAD_REQUEST", reason: "not an app id")
        }
        return root?.appendingPathComponent(app, isDirectory: true)
    }

    /// FsBlobStore's ids are uuids; anything else names no file.
    private static func isId(_ id: String) -> Bool { UUID(uuidString: id) != nil }

    public func put(app: String, name: String, mime: String, bytes: Data) throws -> String {
        let id = UUID().uuidString.lowercased()
        guard let dir = try dir(app) else {
            lock.withLock { memory["\(app)/\(id)"] = bytes }
            return id
        }
        do {
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            try bytes.write(to: dir.appendingPathComponent(id))
            let meta = try JSONSerialization.data(withJSONObject: ["name": name, "mime": mime], options: [.sortedKeys])
            try meta.write(to: dir.appendingPathComponent("\(id).json"))
        } catch {
            throw EngineError.Failure(code: "IO", reason: error.localizedDescription)
        }
        return id
    }

    public func get(app: String, id: String) throws -> Data? {
        guard Self.isId(id) else { return nil }
        guard let dir = try dir(app) else { return lock.withLock { memory["\(app)/\(id)"] } }
        let file = dir.appendingPathComponent(id)
        guard FileManager.default.fileExists(atPath: file.path) else { return nil }
        do { return try Data(contentsOf: file) } catch { throw EngineError.Failure(code: "IO", reason: error.localizedDescription) }
    }

    public func delete(app: String, id: String) throws {
        guard Self.isId(id) else { return }
        guard let dir = try dir(app) else {
            lock.withLock { memory["\(app)/\(id)"] = nil }
            return
        }
        for file in [dir.appendingPathComponent(id), dir.appendingPathComponent("\(id).json")] where FileManager.default.fileExists(atPath: file.path) {
            do { try FileManager.default.removeItem(at: file) } catch { throw EngineError.Failure(code: "IO", reason: error.localizedDescription) }
        }
    }
}
