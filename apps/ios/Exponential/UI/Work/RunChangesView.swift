import ExpCore
import ExpUI
import SwiftUI

/// EXP-1194: the review of a RUN's own issue-less pull request (Reviews →
/// Agent runs, `AppRoute.runChanges`) — the same Changes face an issue row
/// opens (`PrChangesFace`), fed by `codingSessions.prFiles` through
/// `ChangesViewModel(source: .session)`. The title names the run (the Reviews
/// row's title), GitHub rides the nav bar's action edge like the issue
/// Changes face's, and the white Merge capsule (`WorkMergePill`, `.session`
/// target) merges through `codingSessions.mergePr` while the PR is open.
struct RunChangesView: View {
    let sessionId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.openURL) private var openURL
    @State private var model: ChangesViewModel?
    @State private var focusPath: String?
    @State private var mergeState = WorkMergeState()

    private var session: CodingSessionEntity? { model?.session }

    /// The Reviews row's own title: a chat run's subject, an action run's
    /// snapshot, else the branch, else "Chat".
    private var title: String {
        guard let session else { return PastRuns.chatRunName }
        return PastRuns.chatSubject(session)
            ?? session.actionName
            ?? session.branch
            ?? PastRuns.chatRunName
    }

    private var prURL: URL? {
        session?.prUrl.flatMap { $0.isEmpty ? nil : URL(string: $0) }
    }

    private var canMerge: Bool { session?.hasOpenPr == true }

    var body: some View {
        ZStack {
            AppBackground()
            if let model {
                PrChangesFace(
                    model: model,
                    focusPath: $focusPath,
                    showsMerge: canMerge
                ) {
                    if canMerge {
                        WorkMergePill(
                            target: .session(sessionId: sessionId),
                            issue: nil,
                            prIssues: [],
                            // No recovery run: Fix conflicts takes an
                            // issue-linked PR.
                            steerEnabled: false,
                            state: $mergeState
                        )
                    }
                }
            } else {
                ProgressView().tint(.white)
            }
        }
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                if let url = prURL {
                    Button { openURL(url) } label: {
                        AppIcon(
                            AppIcons.uiGithub,
                            size: AppIcon.Size.medium,
                            weight: .medium
                        )
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                    .accessibilityLabel(DomainContract.diffUiOpenOnGithub)
                    .accessibilityIdentifier("changes-github-action")
                }
            }
        }
        .onAppear {
            // Re-arm on every appear: pushing a child screen stops the
            // observation (onDisappear), popping back must resume it.
            if model == nil {
                model = ChangesViewModel(
                    accountId: accountId,
                    source: .session(sessionId),
                    db: deps.db,
                    issuesApi: deps.issuesApi,
                    repositoriesApi: deps.repositoriesApi,
                    codingSessionsApi: deps.codingSessionsApi
                )
            }
            model?.startObserving()
        }
        .onDisappear {
            model?.stopObserving()
        }
    }
}
