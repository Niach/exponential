import ExpPush
import ExpUI
import SwiftUI

/// #EXP-1639: the `os-live-activity` capture specimen (`-uiTesting
/// -uiTestingStyleguide os-live-activity`): the widget extension's views
/// (`SessionActivityViews.swift`, compiled into both) over the fixture's
/// content cases, framed like the OS frames them: the Lock Screen card and
/// the Dynamic Island's compact, minimal and expanded presentations. The OS
/// draws the real ones; an extension cannot be driven by a test.
struct LiveActivitySpecimens: View {
    /// `os-live-activity` = the Lock Screen, `os-live-activity-island` = the
    /// Dynamic Island (split: both do not fit one screen).
    static let kinds: Set<String> = ["os-live-activity", "os-live-activity-island"]

    var kind = "os-live-activity"

    private static let startedAt = Date().addingTimeInterval(-(12 * 60 + 34))

    private static func display(
        phase: String, title: String = "Reduce cold start", subtitle: String,
        detail: String? = nil, badge: LiveActivityDisplay.Badge
    ) -> LiveActivityDisplay {
        LiveActivityDisplay(
            sessionId: "s1", userId: "u1", phase: phase, title: title, subtitle: subtitle,
            detail: detail, badge: badge, startedAt: startedAt
        )
    }

    /// The fixture's `contentCases`, in order.
    static let cases: [LiveActivityDisplay] = [
        display(phase: "working", subtitle: "Working · Mira's MacBook", badge: .timer),
        display(
            phase: "needsYou", subtitle: "Needs you",
            detail: "Question: Which database should we use for the cache?", badge: .needsYou
        ),
        display(phase: "working", title: LiveActivityModel.encryptedTitle, subtitle: "Working · Mira's MacBook", badge: .timer),
        display(phase: "working", subtitle: "Working", badge: .text("PR #2041")),
        display(phase: "blocked", subtitle: "Blocked", detail: "Rate limited", badge: .timer),
        display(phase: "done", subtitle: "Done", badge: .text("PR #2041")),
    ]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if kind == "os-live-activity-island" {
                    label("Dynamic Island · compact")
                    HStack(spacing: 12) {
                        island { compact(Self.cases[0]) }
                        island { compact(Self.cases[1]) }
                    }
                    HStack(spacing: 12) {
                        island { compact(Self.cases[3]) }
                        label("Minimal")
                        island { SessionActivityMark(size: 22).frame(width: 36, height: 36) }
                    }
                    label("Dynamic Island · expanded")
                    expanded(Self.cases[1])
                    expanded(Self.cases[0])
                    expanded(Self.cases[4])
                } else {
                    label("Lock Screen")
                    ForEach(Array(Self.cases.enumerated()), id: \.offset) { _, display in
                        SessionActivityLockScreen(display: display)
                            .background(DesignTokens.Palette.background)
                            .clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
                    }
                }
            }
            .padding(16)
        }
        .background(Color(white: 0.16).ignoresSafeArea())
        .preferredColorScheme(.dark)
    }

    private func label(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .padding(.top, 4)
    }

    private func island<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        content()
            .padding(.horizontal, 8)
            .background(Capsule().fill(.black))
    }

    private func compact(_ display: LiveActivityDisplay) -> some View {
        HStack(spacing: 0) {
            SessionActivityMark(size: 22)
            Spacer(minLength: 72)
            SessionActivityBadge(display: display, compact: true)
        }
        .frame(height: 36)
    }

    private func expanded(_ display: LiveActivityDisplay) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                SessionActivityMark(size: 36)
                Spacer()
                SessionActivityBadge(display: display)
            }
            SessionActivityExpanded(display: display)
        }
        .padding(18)
        .background(RoundedRectangle(cornerRadius: 40, style: .continuous).fill(.black))
    }
}
