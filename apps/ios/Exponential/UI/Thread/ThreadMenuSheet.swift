import ExpCore
import ExpUI
import SwiftUI

/// EXP-1295: the strip's menu on a phone — THE Menu in its SHEET
/// presentation (never an inline popover): the "+" opens it untitled with
/// Link an issue · New issue; an issue tab's long press titles it with the
/// identifier and ends in `Unlink <IDENT>` (destructive). A pick hands the
/// action back; the host runs it once the sheet has dismissed (a push or a
/// second sheet from under a dismissing one is dropped). EXP-1307: the
/// Guide header's `…` opens it untitled with `guideMenuEntries` — Open on
/// GitHub · Close PR without merging (destructive).
struct ThreadMenuSheet: View {
    enum Action: Equatable {
        case linkIssue
        case newIssue
        case unlink
        case openGithub
        case closePr
    }

    static let linkIssueLabel = "Link an issue"
    static let newIssueLabel = "New issue"
    static func unlinkLabel(_ identifier: String) -> String { "Unlink \(identifier)" }

    /// The issue tab's identifier; nil = the "+" (no title, no Unlink).
    let identifier: String?
    /// EXP-1307: the Guide's `…` entries; nil = the strip's menu.
    var guideEntries: [SessionTree.GuideMenuEntry]?
    let onPick: (Action) -> Void

    var body: some View {
        GlassSheetChrome(title: identifier) {
            VStack(spacing: 0) {
                if let guideEntries {
                    ForEach(guideEntries, id: \.self) { entry in
                        switch entry {
                        case .openGithub:
                            row(.openGithub, DomainContract.diffUiOpenOnGithub, icon: AppIcons.uiGithub)
                        case .closePr:
                            row(.closePr, ClosePrCopy.menuItem, icon: AppIcons.prClosed, destructive: true)
                        }
                    }
                } else if let identifier {
                    row(.unlink, Self.unlinkLabel(identifier), icon: AppIcons.uiClose, destructive: true)
                } else {
                    row(.linkIssue, Self.linkIssueLabel, icon: AppIcons.navSearch)
                    row(.newIssue, Self.newIssueLabel, icon: AppIcons.uiAdd)
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.top, identifier == nil ? GlassSheetTokens.headerTopPadding : 0)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("thread-menu-sheet")
        }
    }

    private func row(
        _ action: Action, _ title: String, icon: String, destructive: Bool = false
    ) -> some View {
        Button {
            onPick(action)
        } label: {
            GlassMenuItemLabel(title, icon: icon, destructive: destructive)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("thread-menu-\(Self.testId(action))")
    }

    private static func testId(_ action: Action) -> String {
        switch action {
        case .linkIssue: "link-issue"
        case .newIssue: "new-issue"
        case .unlink: "unlink"
        case .openGithub: "open-github"
        case .closePr: "close-pr"
        }
    }
}
