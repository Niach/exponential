import ExpCore
import ExpUI
import SwiftUI

/// EXP-876: the issues ONE piece of work covers — a multi-issue run (opened
/// from its `EXP-874 +2` title on the Work screen) or a pull request linking
/// several issues (a Reviews row). One `IssueChip` per issue, each opening
/// its issue.
struct CoveredIssuesSheet: View {
    let title: String
    let issues: [IssueEntity]
    let onOpenIssue: (String) -> Void

    /// The Work screen's title, byte-identical with Android.
    static let runTitle = "Issues in this run"
    /// The Reviews row's title.
    static let pullRequestTitle = "In this pull request"

    var body: some View {
        GlassSheetChrome(title: title) {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(issues, id: \.id) { issue in
                    IssueChip(
                        identifier: issue.identifier,
                        title: issue.title,
                        status: IssueStatus.from(issue.status),
                        onTap: { onOpenIssue(issue.id) }
                    )
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        }
        .accessibilityIdentifier("covered-issues-sheet")
    }
}
