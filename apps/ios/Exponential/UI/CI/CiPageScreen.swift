import ExpCore
import ExpUI
import GRDB
import SwiftUI

// #EXP-2172 (fixtures/ci-page.json, view `ci`): THE CI page, a root
// destination across every member team (the phones' More › CI, the iPad
// sidebar's CI row), as the genUI tree web builds (`CiPageView.ciPageTree`,
// phone: true) painted by the core catalog. Everything is synced; the
// repository names come from `repositories.list` per member team (as
// Settings › Repositories) until the repositories shape syncs.

/// The route's screen: the synced rows, the tab and the filter.
struct CiPageScreen: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.openURL) private var openURL

    @State private var input: CiPage.Input?
    @State private var repos: [CiPage.Repo] = []
    @State private var state = CiPageView.State(phone: true)
    @State private var now = Date()

    private var running: Bool { input?.runs.contains { $0.state == "pending" } ?? false }

    var body: some View {
        ZStack {
            AppBackground()
            if let input {
                CiPagePage(
                    input: withClock(input),
                    state: state,
                    onState: { state = $0 },
                    onOpenRun: { pushRoute(.ciRun(accountId: accountId, runId: $0)) },
                    onOpenPr: openPullRequest,
                    onOpenRunner: { _ in pushRoute(.devices(accountId: accountId)) }
                )
            } else {
                SkeletonRows(rows: 4)
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .pageHeader(CiPage.Copy.title, pushed: true)
        .task { await observe() }
        .task { await loadRepos() }
        .task(id: running) {
            // Running durations tick each second, the rest each minute.
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(running ? 1 : 60))
                now = Date()
            }
        }
    }

    private func withClock(_ input: CiPage.Input) -> CiPage.Input {
        var next = input
        next.repos = repos
        let iso = ISO8601DateFormatter()
        iso.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        next.now = iso.string(from: now)
        return next
    }

    private func observe() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let viewer = deps.auth.userId ?? ""
        let values = ValueObservation.tracking { db in try CiPage.fetchRows(db, viewerId: viewer) }
        do {
            for try await next in values.values(in: pool) { input = next }
        } catch {
            if input == nil { input = CiPage.Input(runs: [], jobs: [], repos: [], pullRequests: [], sessions: [], runners: [], viewerId: viewer, now: "") }
        }
    }

    /// `repositories.list` of every member team, one read per team.
    private func loadRepos() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId),
              let teams = try? await pool.read({ db in try TeamEntity.fetchAll(db) })
        else { return }
        var all: [CiPage.Repo] = []
        await withTaskGroup(of: [CiPage.Repo].self) { group in
            for team in teams {
                group.addTask { [deps, accountId] in
                    let rows = (try? await deps.repositoriesApi.list(accountId: accountId, teamId: team.id)) ?? []
                    return rows.map { CiPage.Repo(id: $0.id, fullName: $0.fullName) }
                }
            }
            for await rows in group { all.append(contentsOf: rows) }
        }
        repos = all
    }

    /// A pull request opens its chat (where it was made), else its page.
    private func openPullRequest(_ id: String) {
        Task {
            guard let pool = try? deps.db.pool(forAccountId: accountId),
                  let pr = try? await pool.read({ db in try PullRequestEntity.fetchOne(db, key: id) })
            else { return }
            if let root = pr.rootSessionId,
               (try? await pool.read({ db in try SessionEntity.fetchOne(db, key: root) })) != nil {
                pushRoute(.chat(accountId: accountId, chatId: root))
            } else if let url = URL(string: pr.url) {
                openURL(url)
            }
        }
    }
}

/// The stateless page (also the `ci` specimen).
struct CiPagePage: View {
    let input: CiPage.Input
    let state: CiPageView.State
    var onState: (CiPageView.State) -> Void = { _ in }
    var onOpenRun: (String) -> Void = { _ in }
    var onOpenPr: (String) -> Void = { _ in }
    var onOpenRunner: (String) -> Void = { _ in }

    var body: some View {
        let tree = CiPageView.ciPageTree(input, state: state)
        ScrollView {
            CiSurface(surfaceId: "ci-page", tree: tree) { name, context, payload in
                if let next = CiPageView.stateAfter(state, event: name, payload: payload) {
                    if next != state { onState(next) }
                    return
                }
                switch name {
                case CiPageView.Events.openRun:
                    if case let .string(run)? = context?["run"] { onOpenRun(run) }
                case CiPageView.Events.openPr:
                    if case let .string(pr)? = context?["pr"] { onOpenPr(pr) }
                case CiPageView.Events.openRunner:
                    if case let .string(device)? = context?["device"] { onOpenRunner(device) }
                default: break
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 24)
        }
        .accessibilityIdentifier(CiPage.TestIds.page)
    }
}
