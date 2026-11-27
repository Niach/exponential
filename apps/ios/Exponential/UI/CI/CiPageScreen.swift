import ExpCore
import ExpUI
import GRDB
import SwiftUI

// #EXP-2172 (fixtures/ci-page.json, view `ci`): THE CI page, a root
// destination across every member team (the phones' More › CI, the iPad
// sidebar's CI row), as the genUI tree web builds (`CiPageView.ciPageTree`,
// phone: true) painted by the core catalog. Everything is synced; the
// repository names come from `repositories.list` per member team (as
// Settings › Repositories) until the repositories shape syncs. The iPad
// (regular width) draws the wide tree: trailing rows, the runner chip.

/// The route's screen: the synced rows, the tab and the filter.
struct CiPageScreen: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.openURL) private var openURL
    @Environment(\.horizontalSizeClass) private var sizeClass

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
                    state: drawn,
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

    private var drawn: CiPageView.State {
        var next = state
        next.phone = sizeClass != .regular
        return next
    }

    private func withClock(_ input: CiPage.Input) -> CiPage.Input {
        var next = input
        next.repos = repos
        next.utcOffsetMinutes = TimeZone.current.secondsFromGMT(for: now) / 60
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
        VStack(spacing: 0) {
            tabs
            ScrollView {
                surface(tree)
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .padding(.bottom, 24)
            }
        }
        .accessibilityIdentifier(CiPage.TestIds.page)
    }

    /// #EXP-2169 ("one look"): the host's tab switcher, THE board's segmented
    /// capsule (`GlassSegmentedControl`, hugging, left-aligned, scrolling past
    /// the width), Merge train and Runners with their count after the label
    /// as a muted number (none at zero).
    private var tabs: some View {
        let counts = CiPage.tabCounts(input)
        func count(_ tab: CiPage.Tab) -> Int? {
            let value: Int? = switch tab {
            case .runs: nil
            case .train: counts.train
            case .runners: counts.runners
            }
            return value.flatMap { $0 > 0 ? $0 : nil }
        }
        return ScrollView(.horizontal, showsIndicators: false) {
            GlassSegmentedControl(
                options: CiPage.Tab.allCases,
                selection: state.tab,
                label: { CiPage.Copy.tabs[$0.rawValue] ?? $0.rawValue },
                identifier: { "\(CiPage.TestIds.tabs)-\($0.rawValue)" },
                accessory: { tab in
                    count(tab).map { value in
                        AnyView(
                            Text("\(value)")
                                .font(.subheadline.weight(.medium).monospacedDigit())
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(1)
                        )
                    }
                },
                accessoryGap: 4,
                spokenLabel: { tab in count(tab).map { "\(CiPage.Copy.tabs[tab.rawValue] ?? tab.rawValue) \($0)" } },
                hugging: true,
                onSelect: { tab in
                    var next = state
                    next.tab = tab
                    if next != state { onState(next) }
                }
            )
            .padding(.horizontal, 16)
        }
        .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.top, 6)
        .padding(.bottom, 8)
    }

    private func surface(_ tree: CardJSON) -> some View {
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
    }
}

/// #EXP-2169 ("one look", ci-page-view.json `repositoryId` cases): THE repo
/// page's Runs tab = the CI page's day Sections and rows of ONE repository
/// (`CiPageView.ciRepoRunsTree`), no filter bar; a row opens the run page.
/// Stateless (also the `repo-runs` specimen); the host scrolls.
struct RepoRunsView: View {
    let input: CiPage.Input
    let repositoryId: String
    var phone = true
    var onOpenRun: (String) -> Void = { _ in }

    var body: some View {
        CiSurface(surfaceId: "repo-runs", tree: CiPageView.ciRepoRunsTree(input, repositoryId: repositoryId, phone: phone)) { name, context, _ in
            if name == CiPageView.Events.openRun, case let .string(run)? = context?["run"] { onOpenRun(run) }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(RepoCi.TestIds.runs)
    }
}

/// The Runs tab over the synced CI rows (the CI page's `fetchRows`), the
/// clock ticking each second while one of this repository's runs runs.
struct RepoRunsLive: View {
    let repositoryId: String
    let fullName: String
    var onOpenRun: (String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var input: CiPage.Input?
    @State private var now = Date()

    private var running: Bool {
        input?.runs.contains { $0.repositoryId == repositoryId && $0.state == "pending" } ?? false
    }

    var body: some View {
        Group {
            if let input {
                RepoRunsView(input: withClock(input), repositoryId: repositoryId, phone: sizeClass != .regular, onOpenRun: onOpenRun)
            } else {
                SkeletonRows(rowHeight: 52)
            }
        }
        .task(id: "\(accountId)/\(repositoryId)") { await observe() }
        .task(id: running) {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(running ? 1 : 60))
                now = Date()
            }
        }
    }

    private func withClock(_ input: CiPage.Input) -> CiPage.Input {
        var next = input
        next.repos = [CiPage.Repo(id: repositoryId, fullName: fullName)]
        next.utcOffsetMinutes = TimeZone.current.secondsFromGMT(for: now) / 60
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
}
