import Foundation

// Mirrors apps/web/src/lib/trpc/apps.ts (VAPP-110). The rows sync (`apps`
// carries the head package, `app_versions` the versions' metadata), so a
// write needs no local echo; the returned txid is never needed here.
// Creating = any member; renaming and deleting = the creator or a team
// owner; restoring = any member (a NEW head copying an older version).
// Publishing a version is the editor session's (MCP `apps upsert`).

/// `apps.create` (the slug derives server-side from the name).
public struct AppCreateInput: Encodable, Sendable, Equatable {
    public let teamId: String
    public let name: String
    public let icon: String?
    public let color: String?
    public let description: String?

    public init(teamId: String, name: String, icon: String?, color: String?, description: String?) {
        self.teamId = teamId
        self.name = name
        self.icon = icon
        self.color = color
        self.description = description
    }

    enum CodingKeys: String, CodingKey {
        case teamId, name, icon, color, description
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(teamId, forKey: .teamId)
        try c.encode(name, forKey: .name)
        try c.encodeIfPresent(icon, forKey: .icon)
        try c.encodeIfPresent(color, forKey: .color)
        try c.encodeIfPresent(description, forKey: .description)
    }
}

/// `apps.update` (Rename: the New app form prefilled).
public struct AppUpdateInput: Encodable, Sendable, Equatable {
    public let id: String
    public let name: String
    public let icon: String?
    public let color: String?
    public let description: String

    public init(id: String, name: String, icon: String?, color: String?, description: String) {
        self.id = id
        self.name = name
        self.icon = icon
        self.color = color
        self.description = description
    }
}

/// The created app's handle (the editor opens on it).
public struct AppCreated: Decodable, Sendable, Equatable {
    public let id: String
    public let slug: String
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

    enum CodingKeys: String, CodingKey { case id, kind, data, recordId }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(kind, forKey: .kind)
        try c.encode(data, forKey: .data)
        try c.encodeIfPresent(recordId, forKey: .recordId)
    }
}

private struct AppRecordDeleteInput: Encodable {
    let id: String
    let recordId: String
}

/// `apps.records.upsert`'s answer: the row's id and whether it is new.
public struct AppRecordUpserted: Decodable, Sendable, Equatable {
    public let id: String
    public let created: Bool
}

public final class AppsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Member-gated `apps.create`; the editor opens on the returned slug.
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

    /// `apps.update` — the creator or a team owner.
    public func update(accountId: String, _ input: AppUpdateInput) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "apps.update", input: input)
    }

    /// `apps.delete` (the trash mirror) — the creator or a team owner.
    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "apps.delete", input: AppIdInput(id: id))
    }

    /// `apps.restore`: a NEW head copying version `number`.
    public func restore(accountId: String, id: String, number: Int) async throws -> AppRestoreResult {
        try await trpc.mutation(accountId: accountId, path: "apps.restore", input: AppRestoreInput(id: id, number: number))
    }

    /// VAPP-122 `apps.records.upsert` as the viewer (the app's own rows; the
    /// server validates `data` against the kind's schema and the quota).
    public func recordsUpsert(
        accountId: String, appId: String, kind: String, data: [String: CardJSON], recordId: String?
    ) async throws -> AppRecordUpserted {
        try await trpc.mutation(
            accountId: accountId,
            path: "apps.records.upsert",
            input: AppRecordUpsertInput(id: appId, kind: kind, data: EncodableJSON(value: .object(data)), recordId: recordId)
        )
    }

    /// VAPP-122 `apps.records.delete` (the trash mirror).
    public func recordsDelete(accountId: String, appId: String, recordId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "apps.records.delete", input: AppRecordDeleteInput(id: appId, recordId: recordId)
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
