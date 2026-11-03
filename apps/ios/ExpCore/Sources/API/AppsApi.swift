import Foundation

// Mirrors apps/web/src/lib/trpc/apps.ts (VAPP-110). The rows sync (`apps`
// carries the head package, `app_versions` the versions' metadata), so a
// write needs no local echo; the returned txid is never needed here.
// Creating = any member; renaming and deleting = the creator or a team
// owner; restoring = any member (a NEW head copying an older version).
// Saving a version (a DRAFT) is the editor session's (MCP `apps upsert`);
// eApps V1 (#VAPP-210): `apps.publish` moves the published pointer (write
// access to the app).

/// VAPP-176 `apps.create {teamId}` with NO other field = a DRAFT named
/// "Untitled app" (no version; the editor session names it).
public struct AppCreateInput: Encodable, Sendable, Equatable {
    public let teamId: String

    public init(teamId: String) {
        self.teamId = teamId
    }
}

/// `apps.update` = a PATCH (VAPP-177: the editor's Settings sheet): every
/// field but `id` is optional and an unset one is left out of the JSON (the
/// synthesized `encodeIfPresent`). The identity + description = the creator
/// or a team owner; `grantedTools` = an owner only and names the head
/// version on screen (`headVersion`, VAPP-156: a newer head = CONFLICT).
public struct AppUpdateInput: Encodable, Sendable, Equatable {
    public let id: String
    public let name: String?
    public let icon: String?
    public let color: String?
    public let description: String?
    public let grantedTools: [String]?
    public let headVersion: Int?

    public init(
        id: String,
        name: String? = nil,
        icon: String? = nil,
        color: String? = nil,
        description: String? = nil,
        grantedTools: [String]? = nil,
        headVersion: Int? = nil
    ) {
        self.id = id
        self.name = name
        self.icon = icon
        self.color = color
        self.description = description
        self.grantedTools = grantedTools
        self.headVersion = headVersion
    }
}

/// The created app's handle (the editor opens on it).
public struct AppCreated: Decodable, Sendable, Equatable {
    public let id: String
    public let slug: String
}

/// eApps V1 `apps.publish`'s answer: the published version and the head.
public struct AppPublishResult: Decodable, Sendable, Equatable {
    public let published: Int
    public let head: Int
}

/// `apps.restore`'s answer: the version restored and the new head.
public struct AppRestoreResult: Decodable, Sendable, Equatable {
    public let version: Int
    public let restored: Int
}

private struct AppCreateResult: Decodable {
    let app: AppCreated
}

/// VAPP-121 `apps.validateImport` / `apps.import` input: the file's text.
private struct AppImportInput: Encodable {
    let teamId: String
    let text: String
}

/// VAPP-121 `apps.validateImport`'s answer: the server's issues (JSON
/// pointers into the file), the app's name and the record count.
public struct AppImportCheck: Decodable, Sendable, Equatable {
    public let issues: [ImportIssue]
    public let name: String?
    public let records: Int
}

private struct AppImportResult: Decodable {
    let app: AppCreated
}

private struct AppDiscardResult: Decodable {
    let discarded: Bool
}

private struct AppIdInput: Encodable {
    let id: String
}

private struct AppVersionInput: Encodable {
    let id: String
    let version: Int
}

private struct AppRestoreInput: Encodable {
    let id: String
    let number: Int
}

/// `apps.get`: only the package is read here (as compact JSON text).
private struct AppPackageResult: Decodable {
    let package: AnyJSON?
}

/// Any JSON value, decoded into `CardJSON` (a package is free-form).
struct AnyJSON: Decodable {
    let value: CardJSON

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() {
            value = .null
        } else if let bool = try? c.decode(Bool.self) {
            value = .bool(bool)
        } else if let number = try? c.decode(Double.self) {
            value = .number(number)
        } else if let string = try? c.decode(String.self) {
            value = .string(string)
        } else if let array = try? c.decode([AnyJSON].self) {
            value = .array(array.map(\.value))
        } else {
            value = .object(try c.decode([String: AnyJSON].self).mapValues(\.value))
        }
    }
}

/// A `CardJSON` on the wire (a record's free-form `data`).
private struct EncodableJSON: Encodable {
    let value: CardJSON

    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch value {
        case .null: try c.encodeNil()
        case let .bool(v): try c.encode(v)
        case let .number(v): try c.encode(v)
        case let .string(v): try c.encode(v)
        case let .array(v): try c.encode(v.map(EncodableJSON.init))
        case let .object(v): try c.encode(v.mapValues(EncodableJSON.init))
        }
    }
}

/// VAPP-122 `apps.records.upsert` (`recordId` = update the team's row: a
/// shallow patch of `data`).
private struct AppRecordUpsertInput: Encodable {
    let id: String
    let kind: String
    let data: EncodableJSON
    let recordId: String?
    /// VAPP-188: an on-behalf write's author (owners only; nil = omitted).
    var author: String? = nil
    /// VAPP-215: where the write came from (`editor` = the Data tab; nil = omitted).
    var via: String? = nil

    enum CodingKeys: String, CodingKey { case id, kind, data, recordId, author, via }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(kind, forKey: .kind)
        try c.encode(data, forKey: .data)
        try c.encodeIfPresent(recordId, forKey: .recordId)
        try c.encodeIfPresent(author, forKey: .author)
        try c.encodeIfPresent(via, forKey: .via)
    }
}

private struct AppRecordDeleteInput: Encodable {
    let id: String
    let recordId: String
    /// VAPP-215: `editor` = the Data tab (nil = omitted).
    var via: String? = nil
}

/// VAPP-215 `apps.records.history {id, recordId}`.
private struct AppRecordHistoryInput: Encodable {
    let id: String
    let recordId: String
}

/// VAPP-215 `apps.records.restore {id, recordId, eventId, via}`.
private struct AppRecordRestoreInput: Encodable {
    let id: String
    let recordId: String
    let eventId: String
    let via: String
}

/// VAPP-215 `apps.records.undo {id, eventIds, via}`.
private struct AppRecordUndoInput: Encodable {
    let id: String
    let eventIds: [String]
    let via: String
}

/// One `app_record_events` row on the wire (`createdAt` = ISO text).
private struct AppRecordEventWire: Decodable {
    let id: String
    let op: String
    let actor: String?
    let author: String?
    let via: String?
    let patch: AnyJSON?
    let before: AnyJSON?
    let createdAt: String
}

private struct AppRecordHistoryResult: Decodable {
    let events: [AppRecordEventWire]
}

/// `apps.records.restore` / `apps.records.undo`'s answer: the events the
/// new writes left (what the Undo toast undoes).
public struct AppRecordEventsWritten: Decodable, Sendable, Equatable {
    public let txid: Double?
    public let eventIds: [String]
}

/// `apps.records.delete`'s answer (VAPP-215: the write's event).
public struct AppRecordDeleted: Decodable, Sendable, Equatable {
    public let txid: Double?
    public let eventId: String?
}

/// VAPP-194 `apps.records.batch`: a package action's record writes
/// (`{op: "upsert", kind, data, id?}` | `{op: "delete", id}`, the plan's
/// `writes` passed through unchanged) as ONE transaction.
private struct AppRecordBatchInput: Encodable {
    let id: String
    let writes: EncodableJSON
}

/// `apps.records.batch`'s answer: each write's record id, in order.
public struct AppRecordBatched: Decodable, Sendable, Equatable {
    public let txid: Double?
    public let ids: [String]
    /// VAPP-215: the events the batch wrote (the Undo toast's).
    public let eventIds: [String]?
}

/// `apps.records.upsert`'s answer: the row's id and whether it is new.
public struct AppRecordUpserted: Decodable, Sendable, Equatable {
    public let id: String
    public let created: Bool
    /// VAPP-215: the write's event (the Undo toast's).
    public let eventId: String?
}

public final class AppsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Member-gated `apps.create` (VAPP-176: a draft); the editor opens on
    /// the returned app.
    public func create(accountId: String, _ input: AppCreateInput) async throws -> AppCreated {
        let result: AppCreateResult = try await trpc.mutation(accountId: accountId, path: "apps.create", input: input)
        return result.app
    }

    /// VAPP-121 `apps.validateImport`: the server's issues for a pasted or
    /// chosen app file (the one authority ×4).
    public func validateImport(accountId: String, teamId: String, text: String) async throws -> AppImportCheck {
        try await trpc.mutation(
            accountId: accountId, path: "apps.validateImport", input: AppImportInput(teamId: teamId, text: text)
        )
    }

    /// VAPP-121 `apps.import`: creates the app from the file (v1 noted
    /// `Imported`, its records as mine); the editor opens on it like Create.
    public func importApp(accountId: String, teamId: String, text: String) async throws -> AppCreated {
        let result: AppImportResult = try await trpc.mutation(
            accountId: accountId, path: "apps.import", input: AppImportInput(teamId: teamId, text: text)
        )
        return result.app
    }

    /// VAPP-176 `apps.discardDraft`: leaving an EMPTY draft's editor (no
    /// version, no editor session) deletes it; the server ignores anything
    /// else (not mine, a version, a session). True = it was discarded.
    @discardableResult
    public func discardDraft(accountId: String, id: String) async throws -> Bool {
        let result: AppDiscardResult = try await trpc.mutation(
            accountId: accountId, path: "apps.discardDraft", input: AppIdInput(id: id)
        )
        return result.discarded
    }

    /// `apps.update` (a patch) — the creator or a team owner; grants an
    /// owner only.
    public func update(accountId: String, _ input: AppUpdateInput) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "apps.update", input: input)
    }

    /// `apps.delete` (the trash mirror) — the creator or a team owner.
    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "apps.delete", input: AppIdInput(id: id))
    }

    /// eApps V1 `apps.publish {id, number}`: members and the app's tools run
    /// version `number` from now on (an older one = a rollback).
    public func publish(accountId: String, id: String, number: Int) async throws -> AppPublishResult {
        try await trpc.mutation(accountId: accountId, path: "apps.publish", input: AppRestoreInput(id: id, number: number))
    }

    /// `apps.restore`: a NEW head copying version `number`.
    public func restore(accountId: String, id: String, number: Int) async throws -> AppRestoreResult {
        try await trpc.mutation(accountId: accountId, path: "apps.restore", input: AppRestoreInput(id: id, number: number))
    }

    /// VAPP-122 `apps.records.upsert` as the viewer (the app's own rows; the
    /// server validates `data` against the kind's schema and the quota).
    /// VAPP-188: `author` = an on-behalf write (owners only; nil = omitted).
    public func recordsUpsert(
        accountId: String, appId: String, kind: String, data: [String: CardJSON], recordId: String?,
        author: String? = nil, via: String? = nil
    ) async throws -> AppRecordUpserted {
        try await trpc.mutation(
            accountId: accountId,
            path: "apps.records.upsert",
            input: AppRecordUpsertInput(
                id: appId, kind: kind, data: EncodableJSON(value: .object(data)), recordId: recordId, author: author,
                via: via
            )
        )
    }

    /// VAPP-194 `apps.records.batch` as the viewer: a package action's
    /// writes, all or none (the server checks each against its kind's
    /// schema, the caps and the quota again).
    public func recordsBatch(accountId: String, appId: String, writes: [CardJSON]) async throws -> AppRecordBatched {
        try await trpc.mutation(
            accountId: accountId,
            path: "apps.records.batch",
            input: AppRecordBatchInput(id: appId, writes: EncodableJSON(value: .array(writes)))
        )
    }

    /// VAPP-122 `apps.records.delete` (the trash mirror). VAPP-215: `via`
    /// (`editor` = the Data tab) and the write's event back.
    @discardableResult
    public func recordsDelete(accountId: String, appId: String, recordId: String, via: String? = nil) async throws -> AppRecordDeleted {
        try await trpc.mutation(
            accountId: accountId, path: "apps.records.delete",
            input: AppRecordDeleteInput(id: appId, recordId: recordId, via: via)
        )
    }

    /// VAPP-215 `apps.records.history`: a record's events, newest first.
    public func recordsHistory(accountId: String, appId: String, recordId: String) async throws -> [AppRecords.RecordEvent] {
        let result: AppRecordHistoryResult = try await trpc.query(
            accountId: accountId, path: "apps.records.history", input: AppRecordHistoryInput(id: appId, recordId: recordId)
        )
        return result.events.map { event in
            AppRecords.RecordEvent(
                id: event.id,
                op: event.op,
                actor: event.actor,
                before: event.before?.value.objectValue,
                patch: event.patch?.value.objectValue,
                createdAt: AppRecords.isoTimestamp(event.createdAt)
            )
        }
    }

    /// VAPP-215 `apps.records.restore`: the record as `eventId` left it, as
    /// a new write.
    public func recordsRestore(
        accountId: String, appId: String, recordId: String, eventId: String, via: String
    ) async throws -> AppRecordEventsWritten {
        try await trpc.mutation(
            accountId: accountId, path: "apps.records.restore",
            input: AppRecordRestoreInput(id: appId, recordId: recordId, eventId: eventId, via: via)
        )
    }

    /// VAPP-215 `apps.records.undo`: each event's `before` as a new write.
    @discardableResult
    public func recordsUndo(accountId: String, appId: String, eventIds: [String], via: String) async throws -> AppRecordEventsWritten {
        try await trpc.mutation(
            accountId: accountId, path: "apps.records.undo",
            input: AppRecordUndoInput(id: appId, eventIds: eventIds, via: via)
        )
    }

    /// One version's package as compact JSON text (nil = none): the Versions
    /// list's preview of an older version (the shape never carries it).
    public func package(accountId: String, id: String, version: Int) async throws -> String? {
        let result: AppPackageResult = try await trpc.query(
            accountId: accountId, path: "apps.get", input: AppVersionInput(id: id, version: version)
        )
        guard let value = result.package?.value, value != .null else { return nil }
        return value.compactString
    }
}
