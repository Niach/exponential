import ExpCore
import ExpUI
import SwiftUI

/// The agent panel on the issue detail (web/Linux parity): a plan-state chip, a
/// "Changes" button when a PR exists, a "Cancel" button while a run is in flight,
/// and an inline diff (via `issues.prFiles`). Renders nothing when the issue has
/// no agent activity.
struct MacAgentPanel: View {
    @Environment(MacAppDependencies.self) private var deps
    let model: MacIssueDetailModel
    let issue: IssueEntity
    @State private var showDiff = false

    // Plan states where a run is actively executing (so "Cancel" makes sense).
    private static let busyStates: Set<String> = ["drafting", "planning", "coding", "approved"]

    var body: some View {
        if issue.agentPlanState != nil || issue.prUrl != nil {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 8) {
                    Image(systemName: "cpu").foregroundStyle(Accent.indigo)
                    Text("Agent").font(.subheadline.weight(.semibold))
                    statusChip
                    Spacer()
                    if let wid = model.workspaceId,
                       deps.agentService.canRunInteractive(workspaceId: wid),
                       let state = issue.agentPlanState, Self.busyStates.contains(state) {
                        Button(role: .destructive) {
                            deps.agentService.cancelIssue(workspaceId: wid, issueId: issue.id)
                        } label: {
                            Label("Cancel", systemImage: "stop.circle")
                        }
                        .controlSize(.small)
                        .help("Cancel the agent run for this issue")
                    }
                    if let prUrl = issue.prUrl, let url = URL(string: prUrl) {
                        Button { Platform.open(url) } label: {
                            Label("Changes", systemImage: "arrow.triangle.branch")
                        }
                        .controlSize(.small)
                        .help("View the pull request")
                    }
                }
                if let branch = issue.branch, !branch.isEmpty {
                    Text(branch).font(.caption.monospaced()).foregroundStyle(.secondary)
                }
                if issue.prUrl != nil {
                    DisclosureGroup("Changed files", isExpanded: $showDiff) {
                        MacDiffView(accountId: model.accountId, issueId: issue.id)
                    }
                    .font(.subheadline)
                }
            }
            .padding(12)
            .glassSection()
        }
    }

    private var statusChip: some View {
        let s = agentStatus(issue)
        return Text(s.0)
            .font(.caption.weight(.medium))
            .padding(.horizontal, 8).padding(.vertical, 2)
            .background(s.1.opacity(0.15))
            .foregroundStyle(s.1)
            .clipShape(Capsule())
    }

    private func agentStatus(_ issue: IssueEntity) -> (String, Color) {
        if issue.prState == "merged" { return ("Merged", .purple) }
        if issue.prState == "open" { return ("In review", .green) }
        switch issue.agentPlanState {
        case "drafting", "planning": return ("Planning…", .blue)
        case "awaiting_approval": return ("Plan ready", Accent.indigo)
        case "awaiting_answer": return ("Needs answer", .orange)
        case "approved", "coding": return ("Coding…", .blue)
        default: return ("Agent", Color.secondary)
        }
    }
}

/// The PR diff for an issue, loaded once from `issues.prFiles`. Renders each
/// changed file as a collapsible patch block with +/−/context line coloring.
struct MacDiffView: View {
    @Environment(MacAppDependencies.self) private var deps
    let accountId: String
    let issueId: String
    @State private var files: [PrFile]?
    @State private var loadError: String?
    @State private var loaded = false

    var body: some View {
        Group {
            if let loadError {
                Text(loadError).font(.caption).foregroundStyle(.secondary)
            } else if let files {
                if files.isEmpty {
                    Text("No changes yet.").font(.caption).foregroundStyle(.secondary)
                } else {
                    VStack(alignment: .leading, spacing: 8) {
                        ForEach(files) { fileBlock($0) }
                    }
                }
            } else {
                ProgressView().controlSize(.small)
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

    @ViewBuilder
    private func fileBlock(_ file: PrFile) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 8) {
                Text(file.filename).font(.caption.monospaced()).lineLimit(1).truncationMode(.middle)
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
        return Color.secondary
    }

    private func lineBackground(_ line: String) -> Color {
        if line.hasPrefix("@@") { return .clear }
        if line.hasPrefix("+") { return Color.green.opacity(0.08) }
        if line.hasPrefix("-") { return Color.red.opacity(0.08) }
        return .clear
    }
}
