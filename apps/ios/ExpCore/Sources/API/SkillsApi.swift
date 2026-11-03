import Foundation

// #EXP-1661: mirrors apps/web/src/lib/trpc/skills.ts as far as a phone reads
// it. Skills sync as the `skills` shape minus `body`; `skills.get` is the
// only body read (the read-only skill page). Phones never create or edit.

/// `skills.get`'s row: the shape's columns plus the SKILL.md `body`.
public struct SkillDto: Identifiable, Sendable, Decodable, Equatable {
    public let id: String
    public let teamId: String
    public let name: String
    public let description: String
    public let enabledByDefault: Bool
    public let body: String

    public init(
        id: String,
        teamId: String,
        name: String,
        description: String = "",
        enabledByDefault: Bool = false,
        body: String = ""
    ) {
        self.id = id
        self.teamId = teamId
        self.name = name
        self.description = description
        self.enabledByDefault = enabledByDefault
        self.body = body
    }

    enum CodingKeys: String, CodingKey {
        case id, teamId, name, description, enabledByDefault, body
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decodeIfPresent(String.self, forKey: .teamId) ?? ""
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        enabledByDefault = try c.decodeIfPresent(Bool.self, forKey: .enabledByDefault) ?? false
        body = try c.decodeIfPresent(String.self, forKey: .body) ?? ""
    }
}

private struct SkillResult: Decodable {
    let skill: SkillDto
}

private struct SkillIdInput: Encodable {
    let id: String
}

public final class SkillsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Member-gated `skills.get`: the ONLY body path.
    public func get(accountId: String, id: String) async throws -> SkillDto {
        let result: SkillResult = try await trpc.query(
            accountId: accountId, path: "skills.get", input: SkillIdInput(id: id)
        )
        return result.skill
    }
}
