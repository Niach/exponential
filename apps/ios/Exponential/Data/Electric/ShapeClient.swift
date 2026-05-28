import Foundation
import GRDB
import os

private let initialOffset = "-1"
private let liveTimeoutSeconds: TimeInterval = 60
private let logger = Logger(subsystem: "com.straehhuber.exponential", category: "ShapeClient")

final class ShapeClient<T: Codable & Sendable>: Sendable {
    private let shapeName: String
    private let urlPath: String
    private let baseUrlProvider: @Sendable () -> String?
    private let tokenProvider: @Sendable () -> String?
    private let pool: DatabasePool
    private let onMessages: @Sendable ([ShapeMessage<T>]) async throws -> Void

    private let session: URLSession

    init(
        shapeName: String,
        urlPath: String,
        baseUrlProvider: @escaping @Sendable () -> String?,
        tokenProvider: @escaping @Sendable () -> String?,
        pool: DatabasePool,
        onMessages: @escaping @Sendable ([ShapeMessage<T>]) async throws -> Void
    ) {
        self.shapeName = shapeName
        self.urlPath = urlPath
        self.baseUrlProvider = baseUrlProvider
        self.tokenProvider = tokenProvider
        self.pool = pool
        self.onMessages = onMessages

        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = liveTimeoutSeconds + 30
        self.session = URLSession(configuration: config)
    }

    func run() async throws {
        var backoffMs: UInt64 = 500
        var pendingRefetch = false
        while !Task.isCancelled {
            do {
                guard let baseUrl = baseUrlProvider(), let token = tokenProvider() else {
                    try await Task.sleep(for: .seconds(2))
                    continue
                }
                pendingRefetch = try await pollOnce(baseUrl: baseUrl, token: token, pendingRefetch: pendingRefetch)
                backoffMs = 500
                if pendingRefetch {
                    try await Task.sleep(for: .milliseconds(500))
                }
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                logger.warning("[\(self.shapeName)] error: \(error.localizedDescription)")
                SyncDebug.shared.log("[\(shapeName)] ERR: \(error.localizedDescription)")
                try await Task.sleep(for: .milliseconds(backoffMs))
                backoffMs = min(backoffMs * 2, 30_000)
            }
        }
    }

    /// Returns `true` when a refetch was triggered (409 or inline must-refetch)
    /// and the next call should apply the replacement atomically.
    private func pollOnce(baseUrl: String, token: String, pendingRefetch: Bool) async throws -> Bool {
        let saved = try await pool.read { db in
            try ElectricOffset.fetchOne(db, key: shapeName)
        }
        let isInitial = saved == nil

        var components = URLComponents(string: "\(baseUrl)\(urlPath)")!
        if isInitial {
            components.queryItems = [URLQueryItem(name: "offset", value: initialOffset)]
        } else {
            components.queryItems = [
                URLQueryItem(name: "offset", value: saved!.offset),
                URLQueryItem(name: "handle", value: saved!.handle),
                URLQueryItem(name: "live", value: "true"),
            ]
        }

        var request = URLRequest(url: components.url!)
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Accept")

        let (data, response) = try await session.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw ShapeError.invalidResponse
        }

        logger.info("[\(self.shapeName)] HTTP \(httpResponse.statusCode), \(data.count) bytes, initial=\(isInitial)")
        SyncDebug.shared.log("[\(shapeName)] HTTP \(httpResponse.statusCode), \(data.count)B")
        SyncDebug.shared.reportShape(name: shapeName, httpStatus: httpResponse.statusCode, isLive: !isInitial)

        if httpResponse.statusCode == 409 {
            try await pool.write { db in
                try ElectricOffset.deleteOne(db, key: shapeName)
            }
            // Don't delete table data yet — leave stale rows visible until the
            // next poll re-fetches and replaces them atomically.
            return true
        }

        if httpResponse.statusCode == 401 {
            throw ShapeError.unauthorized
        }

        guard (200...299).contains(httpResponse.statusCode) else {
            throw ShapeError.httpError(httpResponse.statusCode)
        }

        let handle = httpResponse.value(forHTTPHeaderField: "electric-handle")
        let offset = httpResponse.value(forHTTPHeaderField: "electric-offset")

        var messages = decodeMessages(data)

        // Handle inline must-refetch control message in the response body:
        // strip it, clear the offset, and signal a pending refetch so the
        // next poll replaces all rows atomically.
        let hasInlineMustRefetch = messages.contains { if case .mustRefetch = $0 { true } else { false } }
        if hasInlineMustRefetch {
            messages.removeAll { if case .mustRefetch = $0 { true } else { false } }
            try await pool.write { db in
                try ElectricOffset.deleteOne(db, key: shapeName)
            }
            if !messages.isEmpty {
                try await onMessages(messages)
            }
            return true
        }

        // After a 409 / inline must-refetch, this is the re-fetch with fresh
        // data. Prepend .mustRefetch so applyBatch does DELETE + INSERTs in
        // one transaction — ValueObservation never sees an empty table.
        if pendingRefetch {
            messages.insert(.mustRefetch, at: 0)
        }

        let inserts = messages.filter { if case .insert = $0 { true } else { false } }.count
        let updates = messages.filter { if case .update = $0 { true } else { false } }.count
        let partials = messages.filter { if case .partialUpdate = $0 { true } else { false } }.count
        if !messages.isEmpty {
            logger.info("[\(self.shapeName)] \(messages.count) msgs (\(inserts) ins, \(updates) upd, \(partials) partial)")
            SyncDebug.shared.log("[\(shapeName)] \(inserts) ins, \(updates) upd, \(partials) partial")
            try await onMessages(messages)
        }

        if let handle, let offset {
            try await pool.write { db in
                try ElectricOffset(shape: shapeName, handle: handle, offset: offset).save(db)
            }
        }

        return false
    }

    private func decodeMessages(_ data: Data) -> [ShapeMessage<T>] {
        guard !data.isEmpty else { return [] }

        // Electric sends a JSON array of message objects. Parse using JSONSerialization
        // for maximum flexibility, then re-encode individual values for Codable decoding.
        guard let array = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else {
            logger.warning("[\(self.shapeName)] top-level decode failed")
            return []
        }
        return array.compactMap { mapRawDict($0) }
    }

    private func mapRawDict(_ dict: [String: Any]) -> ShapeMessage<T>? {
        let headers = dict["headers"] as? [String: Any] ?? [:]

        if let control = headers["control"] as? String {
            switch control {
            case "up-to-date": return .upToDate
            case "must-refetch": return .mustRefetch
            default: return nil
            }
        }

        guard let operation = headers["operation"] as? String,
              let key = dict["key"] as? String else {
            return nil
        }

        let rawValue = dict["value"] as? [String: Any]

        let decodedValue: T? = {
            guard let value = rawValue else { return nil }
            guard let jsonData = try? JSONSerialization.data(withJSONObject: value) else { return nil }
            if let decoded = try? JSONDecoder().decode(T.self, from: jsonData) {
                return decoded
            }
            let coerced = coerceStringValues(value)
            guard let coercedData = try? JSONSerialization.data(withJSONObject: coerced) else { return nil }
            return try? JSONDecoder().decode(T.self, from: coercedData)
        }()

        switch operation {
        case "insert":
            return decodedValue.map { .insert(key: key, value: $0) }
        case "update":
            if let value = decodedValue {
                return .update(key: key, value: value)
            }
            guard let rawValue else { return nil }
            let coerced = coerceStringValues(rawValue)
            guard let columnData = try? JSONSerialization.data(withJSONObject: coerced) else { return nil }
            return .partialUpdate(key: key, columns: columnData)
        case "delete":
            return .delete(key: key, value: decodedValue)
        default:
            return nil
        }
    }
}

enum ShapeError: Error {
    case invalidResponse
    case unauthorized
    case httpError(Int)
}

// Electric SQL sends all column values as strings in the wire format.
// This function attempts to coerce string values that look like numbers/bools
// into their native JSON types so Codable decoding succeeds.
private func coerceStringValues(_ dict: [String: Any]) -> [String: Any] {
    var result = [String: Any]()
    for (key, value) in dict {
        if let str = value as? String {
            if str == "true" {
                result[key] = true
            } else if str == "false" {
                result[key] = false
            } else if str.contains("."), let d = Double(str) {
                result[key] = d
            } else if let i = Int(str) {
                result[key] = i
            } else {
                result[key] = str
            }
        } else {
            result[key] = value
        }
    }
    return result
}
