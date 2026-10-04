import ExpCore
import ExpUI
import SwiftUI

/// EXP-893/1152: the Changes face while the shown run HAS a live worktree
/// diff — the diff as a full page (`SessionDiffList`) over its bar (the file
/// sheet circle, else GitHub). It used to be a `face` of `AgentSessionView`,
/// one identity switching between transcript and diff; EXP-1152's pager
/// mounts the Run and Changes PAGES side by side, and two session views would
/// both consume the screen's Stop `request` and both report `RunChrome`. So
/// the diff is its own view over the SAME retained model: it takes its own
/// ref-counted claim (`attachSteerModel`) while it is up, so a finished run's
/// model survives whichever page the pager unmounts first.
///
/// EXP-1154: the bar is the centred cluster `[files circle] [Merge PR]`, the
/// centre the host's white Merge capsule (`merge`); the picked file is the
/// screen's (`focusPath`), so a Results file row can pick it too.
struct RunChangesFace<Merge: View>: View {
    let accountId: String
    let session: CodingSessionEntity
    /// The file the sheet (or a Results file row) picked.
    @Binding var focusPath: String?
    /// Whether `merge` draws anything (a generic view cannot say).
    var showsMerge: Bool = false
    @ViewBuilder var merge: () -> Merge

    @Environment(AppDependencies.self) private var deps
    @Environment(\.openURL) private var openURL
    @State private var model: AgentSessionModel?
    /// This view holds ONE claim on the model. The pager can fire `onAppear` /
    /// `onDisappear` unpaired, and the store's count must not drift with it
    /// (a leaked claim keeps a finished run's socket, a lost one reaps a
    /// model still shown).
    @State private var claimed = false
    /// The phone's file list, off the Changes bar's leading slot.
    @State private var diffFileSheet = false

    /// EXP-895: the model's ONE memoised parse (`AgentSessionModel.parsedDiff`)
    /// — the page and the file sheet read the same `Diff.Parsed`.
    private var parsedDiff: Diff.Parsed { model?.parsedDiff ?? Diff.Parsed(files: []) }

    var body: some View {
        VStack(spacing: 0) {
            if let model {
                changesFace(model)
            } else {
                ProgressView().tint(.white)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        // Off the edge, only the file selection has to follow a vanished diff.
        .onChange(of: model?.latestDiff) { _, diff in
            if diff == nil { focusPath = nil }
        }
        .onAppear {
            guard !claimed else { return }
            claimed = true
            model = deps.attachSteerModel(accountId: accountId, session: session)
        }
        .onDisappear {
            guard claimed else { return }
            claimed = false
            deps.steerSessions.detach(accountId: accountId, sessionId: session.id)
        }
    }

    /// The run's latest worktree diff, with the files bar under it. The
    /// screen only routes here while there IS a diff (it falls back to the
    /// issue's PR files, or to the Run face, when it vanishes).
    @ViewBuilder
    private func changesFace(_ model: AgentSessionModel) -> some View {
        if model.latestDiff != nil {
            // EXP-895: the raw `git diff` was read by the ONE parser the
            // moment it landed, so the face draws the same cards every other
            // Changes surface does.
            SessionDiffList(
                files: parsedDiff.files,
                truncatedLines: parsedDiff.truncatedLines,
                focusPath: focusPath
            )
            .safeAreaInset(edge: .bottom, spacing: 0) { changesFaceBar(model) }
            .sheet(isPresented: $diffFileSheet) {
                DiffFileListSheet(
                    files: parsedDiff.files,
                    selected: focusPath,
                    onSelect: { focusPath = $0 }
                )
            }
        } else {
            Spacer()
            changesFaceBar(model)
        }
    }

    /// The PR page a GitHub circle opens — the issue's, or the run's OWN
    /// issue-less one (EXP-734).
    private func prURL(_ model: AgentSessionModel) -> URL? {
        (model.mergeIssue?.prUrl ?? model.session?.prUrl).flatMap { URL(string: $0) }
    }

    /// The file list (else GitHub) beside the host's Merge capsule
    /// (EXP-1154: back from the header band to the bar).
    @ViewBuilder
    private func changesFaceBar(_ model: AgentSessionModel) -> some View {
        if !parsedDiff.files.isEmpty || prURL(model) != nil || showsMerge {
            FloatingBarCluster {
                // EXP-895: the leading slot is the file list. GitHub keeps the
                // slot only where there is no list to put there — an
                // issue-less run has no header action slot to move it to.
                if !parsedDiff.files.isEmpty {
                    DiffFilesBarCircle(count: parsedDiff.files.count) { diffFileSheet = true }
                } else if let url = prURL(model) {
                    FloatingBarCircle(
                        accessibilityLabel: DomainContract.diffUiOpenOnGithub,
                        action: { openURL(url) }
                    ) {
                        AppIcon(AppIcons.uiGithub, size: AppIcon.Size.medium, weight: .medium)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                }
            } center: {
                merge()
            } trailing: {
                EmptyView()
            }
            // EXP-1162: the bottom edge strip, behind the cluster.
            .floatingBarEdge()
        }
    }
}
