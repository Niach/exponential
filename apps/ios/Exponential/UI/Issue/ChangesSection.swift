import ExpUI
import ExpCore
import SwiftUI

/// The issue's "Changes" surface (masterplan §4.8, mobile tiers 2–4). One diff
/// surface per issue, capability-tiered but with the same tab meaning on every
/// client. Mobile never sees a local worktree (tier 1 is desktop-only):
///   2. PR exists            → PR diff (`issues.prFiles`), as today.
///   3. branch pushed, no PR → `repositories.branchDiff` (same file renderer).
///   4. nothing pushed yet   → "Being coded on <deviceLabel> — Watch / Steer"
///                             opening the native steer viewer (when a session
///                             is running); otherwise a quiet empty state.
/// Mobile does no git operations (L18) — this is observe-only.
struct ChangesSection: View {
    let issue: IssueEntity
    let runningSessions: [CodingSessionEntity]

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var branchFiles: [PrFile]?
    @State private var branchLoaded = false
    @State private var showPRDiff = false
    @State private var watchingSession: CodingSessionEntity?
    @State private var steerEnabled = false

    private var session: CodingSessionEntity? {
        runningSessions.max { $0.startedAt < $1.startedAt }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            header

            if issue.prUrl != nil {
                prTier
            } else if let branchFiles, !branchFiles.isEmpty {
                branchTier(branchFiles)
            } else if let session {
                codingTier(session)
            } else if branchLoaded {
                Text("No changes yet.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            } else {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small).tint(.white)
                    Text("Checking for changes…")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
        }
        .padding(12)
        .glassSection()
        .task(id: "\(issue.id)|\(issue.prUrl ?? "")") { await loadBranchDiff() }
        .task {
            steerEnabled = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi).enabled
        }
        .fullScreenCover(item: $watchingSession) { session in
            SteerTerminalView(accountId: accountId, session: session)
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: 8) {
            Image(systemName: "arrow.triangle.branch")
                .foregroundStyle(Accent.indigo)
            Text("Changes")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white)
            if let branch = issue.branch, !branch.isEmpty {
                Text(branch)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer()
            // Watch button in the header whenever a session is running (§4.8).
            if let session, steerEnabled {
                Button {
                    watchingSession = session
                } label: {
                    Label("Watch", systemImage: "play.display")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white)
                }
                .buttonStyle(.plain)
            }
        }
    }

    // MARK: - Tier 2: PR diff

    @ViewBuilder
    private var prTier: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                if let prState = issue.prState {
                    Text(prState.capitalized)
                        .font(.caption2.weight(.medium))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Color.white.opacity(0.08))
                        .clipShape(Capsule())
                        .foregroundStyle(.white)
                }
                Spacer()
                if let prUrl = issue.prUrl, let url = URL(string: prUrl) {
                    Link("Open PR on GitHub", destination: url).font(.caption)
                }
            }
            DisclosureGroup("Changed files", isExpanded: $showPRDiff) {
                DiffView(issueId: issue.id).padding(.top, 6)
            }
            .font(.subheadline)
        }
    }

    // MARK: - Tier 3: pushed branch, no PR

    @ViewBuilder
    private func branchTier(_ files: [PrFile]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Pushed to \(issue.branch ?? "the branch") — no PR yet")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            DiffFilesView(files: files)
        }
    }

    // MARK: - Tier 4: being coded on, nothing pushed

    @ViewBuilder
    private func codingTier(_ session: CodingSessionEntity) -> some View {
        let label = (session.deviceLabel?.isEmpty == false) ? session.deviceLabel! : "a desktop"
        HStack(spacing: 8) {
            Circle()
                .fill(DesignTokens.Semantic.green)
                .frame(width: 8, height: 8)
            Text("Being coded on \(label)")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
            if steerEnabled {
                Button {
                    watchingSession = session
                } label: {
                    Label("Watch / Steer", systemImage: "play.display")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .glassButton()
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func loadBranchDiff() async {
        // Only the middle tier needs a fetch — a PR uses its own diff loader.
        guard issue.prUrl == nil else { return }
        branchLoaded = false
        do {
            branchFiles = try await deps.repositoriesApi.branchDiff(accountId: accountId, issueId: issue.id)?.files
        } catch {
            branchFiles = nil
        }
        branchLoaded = true
    }
}
