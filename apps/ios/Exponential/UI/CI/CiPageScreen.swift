import ExpCore
import ExpUI
import GRDB
import SwiftUI

// #EXP-2172 (fixtures/ci-page.json, view `ci`): THE CI page, a root
// destination across every member team (the phones' More › CI, the iPad
// sidebar's CI row), as the genUI tree web builds (`CiPageView.ciPageTree`,
// phone: true) painted by the core catalog. Everything is synced (#EXP-2214:
// the repositories shape, the merge queue's trains and history too). The
// Merge queue tab opens first. The iPad (regular width) draws the wide tree:
// the queue's stats and columns, trailing rows, the runner chip.

/// The route's screen: the synced rows, the tab and the filter.
struct CiPageScreen: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.horizontalSizeClass) private var sizeClass

    @State private var input: CiPage.Input?
    @State private var state = CiPageView.State(phone: true)
    @State private var now = Date()

    private var running: Bool { input?.runs.contains { $0.state == "pending" } ?? false }

    var body: some View {
        ZStack {
            AppBackground()
            if let input {
                CiEvents { onEvent in
                    CiPagePage(input: CiPage.withClock(input, now), state: drawn, onState: { state = $0 }, onEvent: onEvent)
                }
            } else {
                SkeletonRows(rows: 4)
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .pageHeader(CiPage.Copy.title, pushed: true)
        .task { input = await CiPage.observeRows(deps: deps, accountId: accountId) { input = $0 } }
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
}

extension CiPage {
    /// The input at the viewer's clock (`now`, the UTC offset).
    static func withClock(_ input: Input, _ now: Date) -> Input {
        var next = input
        next.utcOffsetMinutes = TimeZone.current.secondsFromGMT(for: now) / 60
        let iso = ISO8601DateFormatter()
        iso.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        next.now = iso.string(from: now)
        return next
    }

    /// The synced rows (`fetchRows`: runs, jobs, PRs, repositories, the
    /// merge queue) as they change; an unreadable store = an empty page.
    @MainActor
    static func observeRows(deps: AppDependencies, accountId: String, onChange: (Input) -> Void) async -> Input? {
        let viewer = deps.auth.userId ?? ""
        let empty = Input(runs: [], jobs: [], repos: [], pullRequests: [], sessions: [], runners: [], viewerId: viewer, now: "")
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return empty }
        let values = ValueObservation.tracking { db in try fetchRows(db, viewerId: viewer) }
        var last: Input?
        do {
            for try await next in values.values(in: pool) {
                last = next
                onChange(next)
            }
        } catch {
            return last ?? empty
        }
        return last
    }
}

/// #EXP-2214: what the CI trees' presses do, on the CI page and the repo
/// page's Runs tab alike (web `ci-page.tsx` / `repo-ci.tsx`): a run opens its
/// run page, Open log our run page on that job (else the forge's job page),
/// a PR its chat (else its page), a runner the Devices page; the queue's ⋯
/// Merge now / Hold and Resume call `pullRequests.mergeNow` /
/// `pullRequests.hold` (a failure toasts).
struct CiEvents<Content: View>: View {
    @ViewBuilder let content: (_ onEvent: @escaping (String, [String: CardJSON]?, [String: CardJSON]?) -> Void) -> Content

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.openURL) private var openURL
    @Environment(\.toaster) private var toaster

    var body: some View {
        content(handle)
    }

    private func handle(_ name: String, _ context: [String: CardJSON]?, _ payload: [String: CardJSON]?) {
        func value(_ key: String, in map: [String: CardJSON]?) -> String? {
            if case let .string(v)? = map?[key], !v.isEmpty { return v }
            return nil
        }
        let actions = MergeTrainActions(deps: deps, accountId: accountId, toaster: toaster)
        switch name {
        case CiPageView.Events.openRun:
            if let run = value("run", in: context) { pushRoute(.ciRun(accountId: accountId, runId: run)) }
        case CiPageView.Events.openJob:
            if let run = value("run", in: context), let job = value("job", in: context) {
                pushRoute(.ciRun(accountId: accountId, runId: run, job: job))
            }
        case CiPageView.Events.openUrl:
            if let url = value("url", in: context).flatMap(URL.init(string:)) { openURL(url) }
        case CiPageView.Events.openPr:
            if let pr = value("pr", in: context) { openPullRequest(pr) }
        case CiPageView.Events.openRunner:
            pushRoute(.devices(accountId: accountId))
        case CiPageView.Events.queueMenu:
            guard let pr = value("pr", in: context) else { return }
            switch value("value", in: payload) {
            case "merge-now": actions.mergeNow(pullRequestId: pr)
            case "hold": actions.hold(pullRequestId: pr, hold: true)
            default: break
            }
        case CiPageView.Events.resume:
            if let pr = value("pr", in: context) { actions.hold(pullRequestId: pr, hold: false) }
        default:
            break
        }
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

/// The stateless page (also the `ci` specimen). `onEvent` = every press
/// that is not a tab or filter change (`CiEvents`).
struct CiPagePage: View {
    let input: CiPage.Input
    let state: CiPageView.State
    var onState: (CiPageView.State) -> Void = { _ in }
    var onEvent: (String, [String: CardJSON]?, [String: CardJSON]?) -> Void = { _, _, _ in }

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
    /// the width). #EXP-2214: Merge queue · Pull requests · Releases ·
    /// Runners (phones copy.phoneTabs), each count after its label as a
    /// muted number (none at zero).
    private var tabs: some View {
        let counts = CiPage.tabCounts(input)
        let labels = state.phone ? CiPage.Copy.phoneTabs : CiPage.Copy.tabs
        func count(_ tab: CiPage.Tab) -> Int? { counts[tab].flatMap { $0 > 0 ? $0 : nil } }
        return ScrollView(.horizontal, showsIndicators: false) {
            GlassSegmentedControl(
                options: CiPage.Tab.allCases,
                selection: state.tab,
                label: { labels[$0.rawValue] ?? $0.rawValue },
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
            onEvent(name, context, payload)
        }
    }
}

/// #EXP-2214 (ci-page-view.json `repositoryId` cases): THE repo page's Runs
/// tab = a `Segmented` of Merge queue · Pull requests · Releases inside the
/// tree, then that panel of ONE repository (`CiPageView.ciRepoRunsTree`).
/// Stateless (also the `repo-runs` specimen); the host scrolls.
struct RepoRunsView: View {
    let input: CiPage.Input
    let repositoryId: String
    var tab: CiPage.Tab = .train
    var phone = true
    var onTab: (CiPage.Tab) -> Void = { _ in }
    var onEvent: (String, [String: CardJSON]?, [String: CardJSON]?) -> Void = { _, _, _ in }

    var body: some View {
        CiSurface(surfaceId: "repo-runs", tree: CiPageView.ciRepoRunsTree(input, repositoryId: repositoryId, tab: tab, phone: phone)) { name, context, payload in
            if name == CiPageView.Events.tab {
                if case let .string(value)? = payload?["value"], let next = CiPage.Tab(rawValue: value),
                   CiPageView.repoTabs.contains(next) { onTab(next) }
                return
            }
            onEvent(name, context, payload)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(RepoCi.TestIds.runs)
    }
}

/// The Runs tab over the synced CI rows (the CI page's `fetchRows`), the
/// clock ticking each second while one of this repository's runs runs.
struct RepoRunsLive: View {
    let repositoryId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var input: CiPage.Input?
    @State private var tab: CiPage.Tab = .train
    @State private var now = Date()

    private var running: Bool {
        input?.runs.contains { $0.repositoryId == repositoryId && $0.state == "pending" } ?? false
    }

    var body: some View {
        Group {
            if let input {
                CiEvents { onEvent in
                    RepoRunsView(
                        input: CiPage.withClock(input, now), repositoryId: repositoryId, tab: tab,
                        phone: sizeClass != .regular, onTab: { tab = $0 }, onEvent: onEvent
                    )
                }
            } else {
                SkeletonRows(rowHeight: 52)
            }
        }
        .task(id: "\(accountId)/\(repositoryId)") { input = await CiPage.observeRows(deps: deps, accountId: accountId) { input = $0 } }
        .task(id: running) {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(running ? 1 : 60))
                now = Date()
            }
        }
    }
}
