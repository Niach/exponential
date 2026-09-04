import ExpUI
import ExpCore
import SwiftUI

/// The searchable list of candidate issues both issue pickers show (EXP-736):
/// "Duplicate of" and the second stage of "Add relation". Loads once, filters
/// on title + identifier, and commits immediately on tap — the host owns the
/// sheet chrome (and its pinned search field), this owns the rows.
struct IssueCandidateList: View {
    /// Candidate issues (same team, self excluded), newest first.
    let candidates: [IssueEntity]?
    let searchText: String
    /// The empty-state glyph + copy, so each host keeps its own wording.
    let emptyIcon: String
    let emptyHint: String
    let onSelect: (IssueEntity) -> Void

    private var filtered: [IssueEntity] {
        guard let candidates else { return [] }
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return candidates }
        return candidates.filter {
            $0.title.localizedCaseInsensitiveContains(trimmed)
                || ($0.identifier ?? "").localizedCaseInsensitiveContains(trimmed)
        }
    }

    var body: some View {
        if candidates == nil {
            ProgressView()
                .frame(maxWidth: .infinity)
                .padding(.vertical, 40)
        } else if filtered.isEmpty {
            VStack(spacing: 8) {
                AppIcon(emptyIcon, size: AppIcon.Size.xlarge)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Text("No matching issues")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(emptyHint)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 32)
            .padding(.vertical, 32)
        } else {
            // A `ScrollView` of rows, not a `List`: the chrome measures its
            // content, and a List reports an unbounded height (EXP-687).
            LazyVStack(spacing: 2) {
                ForEach(filtered, id: \.id) { issue in
                    Button {
                        onSelect(issue)
                    } label: {
                        HStack(spacing: 10) {
                            AppIcon(IssueStatus.from(issue.status).iconName, size: AppIcon.Size.small)
                                .foregroundStyle(IssueStatus.from(issue.status).color)
                                .frame(width: 24)
                            if let identifier = issue.identifier {
                                Text(identifier)
                                    .font(.caption.monospaced())
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            }
                            Text(issue.title)
                                .font(.subheadline)
                                .foregroundStyle(.white)
                                .lineLimit(1)
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 14)
                        .frame(minHeight: 44)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 6)
            .padding(.bottom, 16)
        }
    }
}
