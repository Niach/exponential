import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: an issue's PR / pushed-branch diff as the Work screen's Changes
/// FACE — the cards over the floating bar. EXP-1154: the review page is gone
/// (a Reviews row opens THIS face), so the face is the diff only: no merge
/// confirm, no close dialog, no GitHub button of its own (the screen's
/// header carries GitHub, its `…` menu Close PR).
///
/// The bar is the centred cluster `[files circle] [Merge PR]` (EXP-916): the
/// leading slot is the phone's FILE SHEET (EXP-895), the centre the host's
/// white Merge capsule (`merge`, the screen's `WorkMergePill`, which hides
/// itself unless the PR is open).
///
/// EXP-952: the Work screen owns the `ChangesViewModel` (its lifecycle is the
/// host's), so the files are loaded before the face was ever opened.
struct PrChangesFace<Merge: View>: View {
    let model: ChangesViewModel
    /// The file the sheet (or a Results file row) picked: the card list
    /// expands it and jumps. Owned by the screen, so the Results face can
    /// set it before switching here.
    @Binding var focusPath: String?
    /// Whether `merge` draws anything (a generic view cannot say).
    var showsMerge: Bool = false
    @ViewBuilder var merge: () -> Merge

    /// EXP-895: the phone's file list, off the bar's leading slot.
    @State private var fileSheet = false

    var body: some View {
        content
            // The floating bar reserves scroll clearance like the issue
            // face's bottom bar.
            .safeAreaInset(edge: .bottom) { bottomBar }
            // EXP-895: the file list. A column beside the cards leaves neither
            // readable on a phone, so it is a bottom sheet off the bar.
            .sheet(isPresented: $fileSheet) {
                DiffFileListSheet(
                    files: model.loadedFiles ?? [],
                    selected: focusPath,
                    onSelect: { focusPath = $0 }
                )
            }
    }

    private var content: some View {
        let files = model.loadedFiles
        // EXP-895: the ONE diff view. EXP-916: every card starts OPEN — only
        // the size rule folds a huge file away.
        return DiffFileList(
            files: files ?? [],
            emptyLabel: files == nil ? nil : "No changed files.",
            focusPath: focusPath,
            accessibilityId: "changes-file-cards",
            // EXP-1162: the Work screen's header band hangs its own strip.
            headerFade: false,
            header: { loadStatus }
        )
    }

    @ViewBuilder
    private var loadStatus: some View {
        switch model.load {
        case .loading:
            HStack(spacing: 8) {
                ProgressView().controlSize(.small).tint(.white)
                Text("Loading changes…")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, 4)
        case let .failed(message):
            Text("Couldn't load changes: \(message)")
                .font(.caption)
                .foregroundStyle(DesignTokens.Semantic.red)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.top, 4)
        case .loaded:
            EmptyView()
        }
    }

    private var hasFiles: Bool { model.loadedFiles?.isEmpty == false }

    /// The bar shows with something to press: the file list, the Merge.
    @ViewBuilder
    private var bottomBar: some View {
        if hasFiles || showsMerge {
            FloatingBarCluster {
                if let files = model.loadedFiles, !files.isEmpty {
                    DiffFilesBarCircle(count: files.count) { fileSheet = true }
                }
            } center: {
                merge()
            } trailing: {
                EmptyView()
            }
            // EXP-1162: the bottom edge strip, behind the cluster.
            .floatingBarEdge()
            // EXP-642: `contain` keeps its buttons queryable.
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("changes-files-bar")
        }
    }
}
