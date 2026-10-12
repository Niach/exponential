import ExpCore
import ExpUI
import SwiftUI

/// EXP-1284: the thread's child menu on a phone — THE Menu in its SHEET
/// presentation, the same bottom sheet Android and web-mobile open (never an
/// inline popover): an issue tab's long press titles it with the issue's
/// identifier and ends in `Unlink <IDENT>` (destructive); the strip's "+"
/// opens it untitled with the three adding rows. A pick hands the action
/// back; the host runs it once the sheet has dismissed (a push or a second
/// sheet from under a dismissing one is dropped).
struct ThreadMenuSheet: View {
    enum Action: Equatable {
        case linkIssue
        case newIssue
        case startSession
        case unlink
    }

    /// The issue tab's identifier; nil = the "+" (no title, no Unlink).
    let identifier: String?
    let onPick: (Action) -> Void

    var body: some View {
        GlassSheetChrome(title: identifier) {
            VStack(spacing: 0) {
                row(.linkIssue, ThreadViews.linkIssueLabel, icon: AppIcons.navSearch)
                row(.newIssue, ThreadViews.newIssueLabel, icon: AppIcons.uiAdd)
                row(.startSession, ThreadViews.startRunLabel, icon: AppIcons.actionRun)
                if let identifier {
                    row(.unlink, ThreadViews.unlinkLabel(identifier), icon: AppIcons.uiClose, destructive: true)
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
        case .startSession: "start-session"
        case .unlink: "unlink"
        }
    }
}
