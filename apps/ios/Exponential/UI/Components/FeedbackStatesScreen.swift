import ExpCore
import ExpUI
import SwiftUI

/// EXP-1275 (EXP-1259 D8-D10): the iOS styleguide's "Feedback states"
/// screen (web `apps/styleguide` entry `feedback-states`, the IDE styleguide
/// and Android's screen draw the same six cards): inline, pending,
/// placeholder, the mark, blocking, errors, each from the REAL primitive.
/// Capture only: the styleguide suite opens it with `-uiTestingStyleguide
/// feedback-states` (`sg_feedback-states`).
struct FeedbackStatesScreen: View {
    @State private var email = "dennis@"

    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    card("Inline") {
                        VStack(spacing: 8) {
                            band("Team prompt") { InlineStatus(FeedbackStates.Inline.saving) }
                            band("Team prompt") { InlineStatus(FeedbackStates.Inline.saved, busy: false) }
                            band("MCP servers") { InlineSpinner() }
                        }
                    }
                    card("Pending action") {
                        HStack(spacing: 8) {
                            GlassSubmitButton("Start") {}
                            GlassSubmitButton(
                                "Start", loading: true, pendingWord: FeedbackStates.Pending.start
                            ) {}
                        }
                    }
                    card("Placeholder") {
                        SkeletonRows()
                    }
                    card("The mark") {
                        VStack(alignment: .leading, spacing: 12) {
                            FeedbackMark(size: CGFloat(FeedbackStates.Mark.styleguideSize))
                            ComposerStartingRow(
                                starting: .init(deviceLabel: "macbook", since: Date().addingTimeInterval(-1.4))
                            )
                            .padding(12)
                            .glassCard()
                        }
                    }
                    card("Blocking") {
                        BlockingOverlay(FeedbackStates.Blocking.switchTeam("Personal"))
                            .frame(height: 160)
                            .clipShape(RoundedRectangle(cornerRadius: DesignTokens.Radius.lg))
                    }
                    card("Errors") {
                        VStack(alignment: .leading, spacing: 6) {
                            GlassTextField("Email", text: $email)
                                .overlay(
                                    RoundedRectangle(cornerRadius: DesignTokens.Radius.md)
                                        .strokeBorder(DesignTokens.Semantic.red, lineWidth: 1)
                                )
                            Text("Enter a full email address")
                                .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                                .foregroundStyle(DesignTokens.Semantic.red)
                        }
                    }
                }
                .padding(16)
            }
        }
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("feedback-states-screen")
        .onAppear {
            // (6) ERRORS: a failed action is THE toast, with Retry.
            Toaster.shared.error(
                FeedbackStates.Errors.title(verb: "save", thing: "the team prompt"),
                description: "Network error",
                action: ToastAction(label: FeedbackStates.Errors.retry) {}
            )
            // EXP-1391 + EXP-1378: the check gate's refusal offers Merge
            // anyway instead of Retry; in front, the Retry one peeks.
            Toaster.shared.error(
                FeedbackStates.Errors.title(verb: "merge", thing: "the PR"),
                description: "Checks are still running on PR #7: \"gate\". \(FeedbackStates.Errors.MergeRefusal.handoff)",
                action: ToastAction(label: FeedbackStates.Errors.MergeRefusal.force) {}
            )
        }
    }

    private func card<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
            content()
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard()
    }

    private func band<Trailing: View>(_ title: String, @ViewBuilder trailing: () -> Trailing) -> some View {
        HStack {
            Text(title)
                .font(.subheadline)
                .foregroundStyle(.white)
            Spacer(minLength: 8)
            trailing()
        }
        .padding(.horizontal, 12)
        .frame(height: 40)
        .background(GlassTokens.fillActive, in: RoundedRectangle(cornerRadius: DesignTokens.Radius.md))
    }
}

/// The styleguide capture seam: `-uiTesting -uiTestingStyleguide <screen>`
/// names the screen the root covers the app with (capture only).
enum UITestingStyleguide {
    static var requested: String? {
        let arguments = ProcessInfo.processInfo.arguments
        guard arguments.contains("-uiTesting"),
              let keyIndex = arguments.firstIndex(of: "-uiTestingStyleguide"),
              arguments.index(after: keyIndex) < arguments.endIndex
        else { return nil }
        return arguments[arguments.index(after: keyIndex)]
    }
}
