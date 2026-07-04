import ExpCore
import ExpUI
import SwiftUI

/// The PR diff for an issue, loaded once from `issues.prFiles`. Renders each
/// changed file as a patch block with +/−/context line coloring. Mirrors the
/// web diff view (apps/web/src/components/diff-view.tsx). Read-only.
struct DiffView: View {
    let issueId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var files: [PrFile]?
    @State private var loadError: String?
    @State private var loaded = false

    var body: some View {
        Group {
            if let loadError {
                Text(loadError).font(.caption).foregroundStyle(.white.opacity(TextOpacity.secondary))
            } else if let files {
                DiffFilesView(files: files)
            } else {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small).tint(.white)
                    Text("Loading changes…").font(.caption).foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
        }
        .task { await load() }
    }

    private func load() async {
        guard !loaded else { return }
        loaded = true
        do {
            files = try await deps.issuesApi.prFiles(accountId: accountId, issueId: issueId).files
        } catch {
            loadError = "Couldn't load changes from GitHub."
        }
    }
}

/// Shared file-diff renderer (masterplan §4.8 — one diff surface reused across
/// the PR-diff tier and the pushed-branch `branchDiff` tier). Renders each
/// changed file as a patch block with +/−/context line coloring.
struct DiffFilesView: View {
    let files: [PrFile]

    var body: some View {
        if files.isEmpty {
            Text("No changes yet.").font(.caption).foregroundStyle(.white.opacity(TextOpacity.secondary))
        } else {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(files) { fileBlock($0) }
            }
        }
    }

    @ViewBuilder
    private func fileBlock(_ file: PrFile) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 8) {
                Text(file.filename)
                    .font(.caption.monospaced()).lineLimit(1).truncationMode(.middle)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer()
                Text("+\(file.additions)").font(.caption2).foregroundStyle(.green)
                Text("−\(file.deletions)").font(.caption2).foregroundStyle(.red)
            }
            if let patch = file.patch, !patch.isEmpty {
                patchView(patch)
            }
        }
        .padding(8)
        .background(Color.white.opacity(0.03))
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    @ViewBuilder
    private func patchView(_ patch: String) -> some View {
        let lines = patch.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                Text(line.isEmpty ? " " : line)
                    .font(.system(.caption2, design: .monospaced))
                    .foregroundStyle(lineColor(line))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(lineBackground(line))
            }
        }
        .textSelection(.enabled)
    }

    private func lineColor(_ line: String) -> Color {
        if line.hasPrefix("@@") { return Accent.indigo }
        if line.hasPrefix("+") { return .green }
        if line.hasPrefix("-") { return .red }
        return .white.opacity(TextOpacity.secondary)
    }

    private func lineBackground(_ line: String) -> Color {
        if line.hasPrefix("@@") { return .clear }
        if line.hasPrefix("+") { return Color.green.opacity(0.08) }
        if line.hasPrefix("-") { return Color.red.opacity(0.08) }
        return .clear
    }
}
