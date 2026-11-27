import Foundation
import GRDB

// MARK: - Repository (#EXP-2171)

// The team's repositories (the repositories shape, member-only by team,
// archived rows never sync), mirroring the shape allowlist
// `REPOSITORY_COLUMNS`: the Teams tab lists them as items
// (sidebar-tree.json `teamItems`, `repositoryRow`) and repo pages resolve
// their route `slug` against them. `sync` (jsonb) is stored as its JSON
// text. Every non-PK field decodes permissively.
public struct RepositoryEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "repositories"

    public let id: String
    public let teamId: String
    /// `github` | `ecode`.
    public let provider: String
    public let fullName: String
    /// The route key (repo-page.json `slug`), unique per team.
    public let slug: String
    public let description: String?
    /// jsonb `{mode: none|mirror|import, url?, lastSyncedAt?, lastError?}`.
    public let sync: String?
    public let defaultBranch: String
    public let defaultBranchOverride: String?
    public let isPrivate: Bool
    public let inaccessibleAt: String?
    public let sharedByUserId: String?
    public let sortOrder: Double
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String, teamId: String, provider: String = "github", fullName: String, slug: String = "",
        description: String? = nil, sync: String? = nil, defaultBranch: String = "main",
        defaultBranchOverride: String? = nil, isPrivate: Bool = false, inaccessibleAt: String? = nil,
        sharedByUserId: String? = nil, sortOrder: Double = 0, createdAt: String = "", updatedAt: String = ""
    ) {
        self.id = id
        self.teamId = teamId
        self.provider = provider
        self.fullName = fullName
        self.slug = slug
        self.description = description
        self.sync = sync
        self.defaultBranch = defaultBranch
        self.defaultBranchOverride = defaultBranchOverride
        self.isPrivate = isPrivate
        self.inaccessibleAt = inaccessibleAt
        self.sharedByUserId = sharedByUserId
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, provider, slug, description, sync
        case teamId = "team_id"
        case fullName = "full_name"
        case defaultBranch = "default_branch"
        case defaultBranchOverride = "default_branch_override"
        case isPrivate = "private"
        case inaccessibleAt = "inaccessible_at"
        case sharedByUserId = "shared_by_user_id"
        case sortOrder = "sort_order"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    /// `ecode` = Exponential hosts it.
    public var isEcode: Bool { provider == "ecode" }

    /// The row's title (`repositoryRow.nameCases`).
    public var name: String { SidebarTree.repositoryName(fullName) }

    /// The effective default branch (the team's pin wins).
    public var branch: String { defaultBranchOverride ?? defaultBranch }

    /// Copy clone URL (`repositoryRow.cloneUrl`).
    public var cloneUrl: String { SidebarTree.repositoryCloneUrl(provider: provider, fullName: fullName) }
}

extension RepositoryEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decodeIfPresent(String.self, forKey: .teamId) ?? ""
        provider = try c.decodeIfPresent(String.self, forKey: .provider) ?? "github"
        fullName = try c.decodeIfPresent(String.self, forKey: .fullName) ?? ""
        slug = try c.decodeIfPresent(String.self, forKey: .slug) ?? ""
        description = try c.decodeIfPresent(String.self, forKey: .description)
        sync = c.decodeWireJsonString(forKey: .sync)
        defaultBranch = try c.decodeIfPresent(String.self, forKey: .defaultBranch) ?? "main"
        defaultBranchOverride = try c.decodeIfPresent(String.self, forKey: .defaultBranchOverride)
        isPrivate = c.decodeWireBool(forKey: .isPrivate, default: false)
        inaccessibleAt = try c.decodeIfPresent(String.self, forKey: .inaccessibleAt)
        sharedByUserId = try c.decodeIfPresent(String.self, forKey: .sharedByUserId)
        sortOrder = (try? c.decodeWireDouble(forKey: .sortOrder)) ?? 0
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt) ?? ""
        updatedAt = try c.decodeIfPresent(String.self, forKey: .updatedAt) ?? ""
    }
}
