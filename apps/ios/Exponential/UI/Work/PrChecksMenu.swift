import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1611 (pr-checks.json CHECKS LIST): the chat header `…`'s Checks
/// submenu, phones = a page PUSHED in THE Menu's sheet. The entry and each
/// row are THE menu item (`GlassMenuItemLabel`): glyph tinted by its tone
/// (`ci-running` = THE spinner), the check's name, the duration as the
/// value; a pick opens the logs (details_url), a row without one is disabled.
enum PrChecksDisplay {
    static func icon(_ glyph: PrChecks.Glyph) -> String {
        switch glyph {
        case .queued: AppIcons.ciQueued
        case .running: AppIcons.ciRunning
        case .passed: AppIcons.ciPassed
        case .failed: AppIcons.ciFailed
        case .skipped: AppIcons.ciSkipped
        }
    }

    /// nil = muted (the menu's default glyph colour).
    static func tint(_ tone: PrChecks.Tone) -> Color? {
        switch tone {
        case .red: DesignTokens.Semantic.red
        case .green: DesignTokens.Semantic.green
        case .muted: nil
        }
    }
}

/// The Checks entry: summary glyph (tinted), `Checks`, the summary value.
struct PrChecksMenuEntryLabel: View {
    let summary: PrChecks.Summary

    var body: some View {
        GlassMenuItemLabel(
            PrChecks.Copy.checks,
            icon: PrChecksDisplay.icon(summary.glyph),
            iconTint: PrChecksDisplay.tint(summary.tone),
            spinner: summary.glyph == .running,
            value: summary.value,
            chevron: true
        )
    }
}

/// The submenu's rows, in `checksList` order.
struct PrChecksMenuRows: View {
    let rows: [PrChecks.ListRow]
    @Environment(\.openURL) private var openURL
    @Environment(AppDependencies.self) private var deps: AppDependencies?

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                let url = row.url.flatMap(URL.init(string:))
                Button {
                    guard let url else { return }
                    // #EXP-1804 (ci-run.json): our own CI's Details link opens
                    // THE run page in the app (an account on its host); any
                    // other check's logs open in the browser.
                    if let deps, CiRun.parsePath(url.path) != nil {
                        LinkOpener.openWebLink(url, deps: deps)
                    } else {
                        openURL(url)
                    }
                } label: {
                    GlassMenuItemLabel(
                        row.name,
                        icon: PrChecksDisplay.icon(row.glyph),
                        enabled: url != nil,
                        iconTint: PrChecksDisplay.tint(row.tone),
                        spinner: row.glyph == .running,
                        value: row.duration
                    )
                }
                .buttonStyle(.plain)
                .disabled(url == nil)
                .accessibilityIdentifier(PrChecks.TestIds.checkRow)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(PrChecks.TestIds.checksMenu)
    }
}

/// The pushed page's back row naming the submenu (the composer's pushed
/// submenus' shape).
struct PrChecksMenuBackRow: View {
    let onBack: () -> Void

    var body: some View {
        Button(action: onBack) {
            HStack(spacing: 8) {
                AppIcon(AppIcons.uiChevronLeft, size: 14)
                Text(PrChecks.Copy.checks)
                    .font(.headline)
                Spacer(minLength: 0)
            }
            .foregroundStyle(.white)
            .padding(.horizontal, GlassMenuTokens.itemHPadding)
            .padding(.bottom, GlassSheetTokens.headerBottomPadding)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Back")
        .accessibilityIdentifier("pr-checks-menu-back")
    }
}
