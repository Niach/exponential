import ExpUI
import SwiftUI

/// #EXP-1641: the `os-notification-actions` capture specimen (`-uiTesting
/// -uiTestingStyleguide os-notification-actions`): a MOCK of the expanded
/// notification the OS draws for each kind of push, its actions in the
/// fixture's order and copy (notification-actions.json `actionCases`). The OS
/// draws the real ones; a notification cannot be expanded by a test.
struct NotificationActionsSpecimen: View {
    static let kinds: Set<String> = ["os-notification-actions"]

    /// notification-actions.json `copy`.
    private enum Copy {
        static let approve = "Approve"
        static let requestChanges = "Request changes…"
        static let answer = "Answer…"
        static let reply = "Reply"
        static let open = "Open"
    }

    private struct Sample {
        let title: String
        let body: String
        let actions: [String]
    }

    private static let samples = [
        Sample(
            title: "Reduce cold start",
            body: "Question: Which database should we use for the cache?",
            actions: ["Postgres", "SQLite", Copy.answer]
        ),
        Sample(
            title: "Reduce cold start",
            body: "Plan: Cache the parsed config and lazy-load the editor",
            actions: [Copy.approve, Copy.requestChanges, Copy.open]
        ),
        Sample(title: "Release notes", body: "Mira: @you can you check the wording?", actions: [Copy.reply]),
    ]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                ForEach(Array(Self.samples.enumerated()), id: \.offset) { _, sample in
                    VStack(spacing: 8) {
                        notification(sample)
                        actions(sample.actions)
                    }
                }
            }
            .padding(16)
        }
        .background(Color(white: 0.16).ignoresSafeArea())
        .preferredColorScheme(.dark)
    }

    private func notification(_ sample: Sample) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image("AppMark")
                .resizable()
                .frame(width: 38, height: 38)
                .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                HStack {
                    Text(sample.title).font(.system(size: 15, weight: .semibold))
                    Spacer()
                    Text("now")
                        .font(.system(size: 13))
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                Text(sample.body)
                    .font(.system(size: 15))
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                    .lineLimit(3)
            }
        }
        .foregroundStyle(.white)
        .padding(14)
        .background(RoundedRectangle(cornerRadius: 22, style: .continuous).fill(Color(white: 0.24)))
    }

    private func actions(_ titles: [String]) -> some View {
        VStack(spacing: 0) {
            ForEach(Array(titles.enumerated()), id: \.offset) { index, title in
                if index > 0 { Divider().overlay(Color.white.opacity(0.12)) }
                Text(title)
                    .font(.system(size: 17))
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16)
                    .frame(height: 48)
            }
        }
        .background(RoundedRectangle(cornerRadius: 16, style: .continuous).fill(Color(white: 0.24)))
    }
}
