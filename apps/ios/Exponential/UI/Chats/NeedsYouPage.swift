import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// #FEED-191 NEEDS YOU, the phone's page (needs-you.json THE LIST): pushed from
/// the Chats list's first row, the page header with the back control (the
/// Archived page's), one reading column of THE inbox page's rows, the ACTIVE
/// team's. A press opens the chat by the row's `opens`. #EXP-1710: a stuck
/// PR's Button sm row runs Retry (= Resume), Merge now (owners, THE
/// `merge-now-pr` alert, `titleStack` in an open stack) and Close PR
/// (close-pr.json's confirm); an `approval` ready PR's press is Needs
/// approval (THE pill's plain or stack confirm; Merge queues it, Cancel
/// leaves the row), never the chat.
struct NeedsYouPage: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(SessionListsModel.self) private var lists
    @Environment(TeamState.self) private var teamState
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.toaster) private var toaster

    /// The clock the ages read.
    @State private var now = Date()
    @State private var mergeNowTarget: MergeNowTarget?
    @State private var approveTarget: NeedsApprovalTarget?
    @State private var closeTarget: CloseTarget?

    private struct CloseTarget: Identifiable {
        let pullRequestId: String
        let linkedIssues: Int
        var id: String { pullRequestId }
    }

    private var actions: MergeTrainActions {
        MergeTrainActions(deps: deps, accountId: accountId, toaster: toaster)
    }

    var body: some View {
        ZStack {
            AppBackground()
            if lists.loaded {
                NeedsYouPageContent(
                    rows: lists.needsYou(teamId: teamState.activeTeamId, me: deps.auth.userId, now: now),
                    onOpen: { row in
                        if row.needsApproval {
                            askApproval(row)
                        } else {
                            Task { pushRoute(await AppRoute.openingNeedsYou(row, accountId: accountId, deps: deps)) }
                        }
                    },
                    onAction: { row, action in run(action, on: row) }
                )
            } else {
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .pageHeader(NeedsYou.Copy.entry, pushed: true)
        .mergeNowConfirm(target: $mergeNowTarget) { target in
            actions.mergeNow(pullRequestId: target.pullRequestId)
        }
        .needsApprovalConfirm(target: $approveTarget) { target in
            actions.approve(
                pullRequestId: target.stackConfirm?.throughId ?? target.pullRequestId,
                stack: target.stackConfirm != nil
            )
        }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $closeTarget) { target in
                    GlassAlert(
                        title: ClosePrCopy.title,
                        message: ClosePrCopy.message(otherLinkedIssues: max(target.linkedIssues - 1, 0)),
                        actions: [
                            GlassAlertAction(PullRequestStack.cancelLabel, role: .outline, isCancel: true, id: "cancel") {},
                            GlassAlertAction(ClosePrCopy.confirm, role: .destructive, id: "close-pr") {
                                actions.close(pullRequestId: target.pullRequestId)
                            },
                        ]
                    )
                }
        }
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                now = Date()
            }
        }
    }

    /// The row's PR off the synced store (the row's id IS the PR's): the
    /// PR, its open-stack confirm over the team's PRs and its linked issues.
    private struct Found {
        let pullRequest: PullRequestEntity
        let stackConfirm: PullRequestStack.Confirm?
        let mergeNow: MergeNowTarget
        let linkedIssues: Int
    }

    private func load(_ row: NeedsYou.Row, then use: @escaping @MainActor (Found) -> Void) {
        let pullRequestId = row.messageId
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        Task {
            let read = try? await pool.read { db -> Found? in
                guard let pr = try PullRequestEntity.fetchOne(db, key: pullRequestId) else { return nil }
                let team = try PullRequestEntity.filter(Column("team_id") == pr.teamId).fetchAll(db)
                let links = try PullRequestIssueEntity.filter(Column("pull_request_id") == pr.id).fetchCount(db)
                return Found(
                    pullRequest: pr,
                    stackConfirm: pr.isOpen ? PullRequestStack.stackConfirm(pr, in: team) : nil,
                    mergeNow: MergeNowTarget(pr, pool: team),
                    linkedIssues: links
                )
            }
            guard let found = read ?? nil else { return }
            use(found)
        }
    }

    /// An `approval` ready row's press: THE pill's Needs approval confirm.
    private func askApproval(_ row: NeedsYou.Row) {
        load(row) { found in
            approveTarget = NeedsApprovalTarget(
                pullRequestId: found.pullRequest.id,
                number: found.pullRequest.number,
                issueCount: found.linkedIssues,
                stackConfirm: found.stackConfirm
            )
        }
    }

    /// A stuck row's buttons.
    private func run(_ action: MergeTrain.StuckRow.Action, on row: NeedsYou.Row) {
        switch action {
        case .retry:
            actions.hold(pullRequestId: row.messageId, hold: false)
        case .mergeNow:
            load(row) { found in mergeNowTarget = found.mergeNow }
        case .closePr:
            load(row) { found in
                closeTarget = CloseTarget(pullRequestId: found.pullRequest.id, linkedIssues: found.linkedIssues)
            }
        }
    }
}

/// THE list in its scroll; a pure painter (the `needs-you` specimen too).
struct NeedsYouPageContent: View {
    let rows: [NeedsYou.Row]
    let onOpen: (NeedsYou.Row) -> Void
    var onAction: ((NeedsYou.Row, MergeTrain.StuckRow.Action) -> Void)?

    var body: some View {
        if rows.isEmpty {
            NeedsYouList(rows: [], onOpen: onOpen)
                .frame(maxHeight: .infinity)
        } else {
            ScrollView {
                NeedsYouList(rows: rows, onOpen: onOpen, onAction: onAction)
                    .padding(.horizontal, 16)
                    .padding(.top, 4)
                    .padding(.bottom, 16)
            }
            .scrollContentBackground(.hidden)
            .tabBarBottomInset()
        }
    }
}
