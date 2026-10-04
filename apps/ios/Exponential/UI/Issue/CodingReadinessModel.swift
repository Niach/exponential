import ExpCore
import Foundation
import GRDB

/// EXP-1121: the INPUTS of the "Ready to code?" checklist for one issue —
/// the iOS twin of web's `use-coding-readiness.ts`. Devices + users arrive
/// LIVE off the synced shapes (so a machine coming online ticks the row
/// while the sheet is open); the team's repositories and GitHub connection
/// are server-only (tRPC), read when the board changes. The pure,
/// fixture-locked `CodingReadiness.derive` turns them into the steps.
///
/// Owned by the Work screen; the Start circle and the sheet read the SAME
/// instance, so they can never disagree about what is missing.
@MainActor @Observable
final class CodingReadinessModel {
    /// The team's repositories; nil until the first answer (or a failure).
    private(set) var repos: [TeamRepo]?
    private var reposFailed = false
    private var githubStatus: GithubStatusResult?
    private var githubFailed = false
    /// Raw synced rows; nil until the first emission.
    private var deviceRows: [DeviceEntity]?
    private var users: [UserEntity] = []
    /// Re-stamped every 30s — online-ness is a window against `now`.
    private(set) var now = Date()

    private let accountId: String
    private let userId: String?
    private let db: DatabaseManager
    private let repositoriesApi: RepositoriesApi
    private let integrationsApi: IntegrationsApi
    private var teamId: String?
    /// The board id a relist already ran for (web `relistedFor`).
    private var relistedFor: String?
    private var deviceTask: Task<Void, Never>?
    private var tickTask: Task<Void, Never>?

    init(
        accountId: String,
        userId: String?,
        db: DatabaseManager,
        repositoriesApi: RepositoriesApi,
        integrationsApi: IntegrationsApi
    ) {
        self.accountId = accountId
        self.userId = userId
        self.db = db
        self.repositoriesApi = repositoriesApi
        self.integrationsApi = integrationsApi
    }

    // MARK: - Lifecycle

    /// Arm the device observation + the clock for `teamId` (idempotent for
    /// the same team; a new team drops the previous team's answers).
    func start(teamId: String) {
        if self.teamId != teamId {
            self.teamId = teamId
            repos = nil
            reposFailed = false
            githubStatus = nil
            githubFailed = false
            relistedFor = nil
        }
        startObserving()
    }

    func stop() {
        deviceTask?.cancel()
        deviceTask = nil
        tickTask?.cancel()
        tickTask = nil
    }

    private func startObserving() {
        guard deviceTask == nil, let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db -> ([DeviceEntity], [UserEntity]) in
            (try DeviceEntity.fetchAll(db), try UserEntity.fetchAll(db))
        }
        deviceTask = Task { [weak self] in
            do {
                for try await (rows, users) in observation.values(in: pool) {
                    guard let self else { return }
                    self.deviceRows = rows
                    self.users = users
                }
            } catch {}
        }
        tickTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(30))
                guard !Task.isCancelled else { return }
                self?.now = Date()
            }
        }
    }

    // MARK: - Server reads

    /// Re-list the team's repositories, and ask GitHub while the board has
    /// none (after a pick, an add, a trip through Settings).
    func refresh(board: BoardEntity?) async {
        guard let teamId else { return }
        await loadRepos(teamId: teamId)
        if let board, board.repositoryId == nil {
            await loadGithub(teamId: teamId)
        }
    }

    /// The board edge: first answers, the GitHub probe only while the board
    /// has no repository, and ONE relist for a repository id this copy of the
    /// list has never seen (added a moment ago from the picker or Settings).
    func boardChanged(_ board: BoardEntity?) async {
        guard let teamId, let board else { return }
        if repos == nil && !reposFailed {
            await loadRepos(teamId: teamId)
        } else if let repositoryId = board.repositoryId,
                  let repos, !repos.contains(where: { $0.id == repositoryId }),
                  relistedFor != repositoryId {
            relistedFor = repositoryId
            await loadRepos(teamId: teamId)
        }
        if board.repositoryId == nil, githubStatus == nil, !githubFailed {
            await loadGithub(teamId: teamId)
        }
    }

    private func loadRepos(teamId: String) async {
        do {
            let list = try await repositoriesApi.list(accountId: accountId, teamId: teamId)
            guard self.teamId == teamId else { return }
            repos = list
            reposFailed = false
        } catch {
            guard self.teamId == teamId else { return }
            reposFailed = true
        }
    }

    private func loadGithub(teamId: String) async {
        do {
            let status = try await integrationsApi.githubStatus(
                accountId: accountId, teamId: teamId, mobile: true
            )
            guard self.teamId == teamId else { return }
            githubStatus = status
            githubFailed = false
        } catch {
            guard self.teamId == teamId else { return }
            githubFailed = true
        }
    }

    // MARK: - The model

    /// Own devices + the servers shared with the team, online or not.
    private var devices: [CodingReadiness.Device]? {
        guard let deviceRows, let teamId else { return nil }
        return DeviceQueries.compose(
            rows: deviceRows, users: users, teamId: teamId, userId: userId, now: now
        ).map { device in
            CodingReadiness.Device(
                label: device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel,
                own: device.isMine,
                online: device.isOnline,
                lastSeenAtMs: device.lastSeenAt
                    .flatMap(WireTimestamps.parse)
                    .map { Int64(($0.timeIntervalSince1970 * 1000).rounded()) }
            )
        }
    }

    /// `octocat`: the viewer's linked GitHub login, else the first installed
    /// account, else the owner half of the first team repository.
    private func githubLabel(_ repos: [TeamRepo]) -> String? {
        if let login = githubStatus?.login, !login.isEmpty {
            return login
        }
        if let login = githubStatus?.installations.lazy.compactMap(\.accountLogin).first {
            return login
        }
        let owner = repos.first?.fullName.split(separator: "/").first.map(String.init)
        return owner?.isEmpty == false ? owner : nil
    }

    /// Asked only while the board has none; nil keeps the checklist loading.
    private func githubInput(board: BoardEntity?) -> CodingReadiness.Github? {
        guard let board, board.repositoryId == nil else { return nil }
        let reposKnown = repos != nil || reposFailed
        let statusKnown = githubStatus != nil || githubFailed
        guard reposKnown, statusKnown else { return nil }
        let teamRepos = repos ?? []
        // SLOP-7: "GitHub connected" = the viewer's own GitHub account is
        // linked with a live token, or the team already has repositories (a
        // teammate connected them — this person needs no GitHub of their own
        // to pick one).
        let linked = githubStatus?.linked == true && githubStatus?.needsReconnect != true
        return CodingReadiness.Github(
            connected: !teamRepos.isEmpty || linked,
            label: githubLabel(teamRepos)
        )
    }

    func readiness(
        isMember: Bool,
        remoteStartEnabled: Bool?,
        teamName: String,
        board: BoardEntity?
    ) -> CodingReadiness.Readiness {
        let boardRepository: String? = board?.repositoryId.map { id in
            repos?.first { $0.id == id }?.fullName ?? ""
        }
        return CodingReadiness.derive(CodingReadiness.Input(
            isMember: isMember,
            remoteStartEnabled: remoteStartEnabled,
            teamName: teamName,
            boardName: board?.name ?? "",
            boardRepository: boardRepository,
            github: githubInput(board: board),
            devices: devices,
            nowMs: Int64((now.timeIntervalSince1970 * 1000).rounded())
        ))
    }

    /// The picker's rows for `board` (matches first, tags), filtered by `query`.
    func pickerRows(board: BoardEntity, query: String) -> [CodingReadinessRepoPicker.Row] {
        CodingReadinessRepoPicker.rows(
            repos: (repos ?? []).map(CodingReadinessRepoPicker.Repo.init),
            boardId: board.id,
            boardName: board.name,
            boardSlug: board.slug,
            query: query
        )
    }
}
