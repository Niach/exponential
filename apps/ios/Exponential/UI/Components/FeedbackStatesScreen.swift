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

    /// The mark frozen at four phases of its loop (web `MARK_PHASES`).
    private static let markPhases: [Double] = [0.08, 0.2, 0.32, 0.5]

    // EXP-1554: ONE layout ×4 = web's numbered cards (`feedback-specimen.tsx`)
    // in its order and copy; a phone stacks them in one column. The error
    // toast is a card IN its kind, never a live toast over the screen.
    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    card(1, "Inline") {
                        VStack(spacing: 8) {
                            band("Team prompt") { InlineStatus(FeedbackStates.Inline.saving) }
                            band("Team prompt") { InlineStatus(FeedbackStates.Inline.saved, busy: false) }
                            GlassSectionHeader("MCP servers", loading: true)
                        }
                    }
                    card(2, "Pending action") {
                        HStack(spacing: 8) {
                            pendingButton("Start", loading: false)
                            pendingButton("Start", loading: true)
                            GlassPill(FeedbackStates.Pending.connect, size: .md) { InlineSpinner() }
                                .fixedSize()
                            Spacer(minLength: 0)
                        }
                    }
                    card(3, "Placeholder") {
                        SkeletonRows()
                    }
                    card(4, "The mark") {
                        VStack(alignment: .leading, spacing: 12) {
                            HStack(spacing: 16) {
                                ForEach(Self.markPhases, id: \.self) { phase in
                                    FeedbackMark(size: CGFloat(FeedbackStates.Mark.styleguideSize), phase: phase)
                                }
                            }
                            ComposerStartingRow(
                                starting: .init(deviceLabel: "macbook", since: Date().addingTimeInterval(-1.4))
                            )
                            .padding(12)
                            .glassCard()
                        }
                    }
                    card(5, "Blocking") {
                        BlockingOverlay(FeedbackStates.Blocking.switchTeam("Personal"))
                            .frame(height: 160)
                            .clipShape(RoundedRectangle(cornerRadius: DesignTokens.Radius.lg))
                    }
                    card(6, "Errors") {
                        VStack(alignment: .leading, spacing: 12) {
                            ToastCard(
                                item: ToastItem(
                                    kind: .error,
                                    title: FeedbackStates.Errors.title(verb: "save", thing: "the team prompt"),
                                    description: "Network error",
                                    action: ToastAction(label: FeedbackStates.Errors.retry) {}
                                ),
                                onDismiss: {}
                            )
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
                }
                .padding(16)
            }
        }
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("feedback-states-screen")
    }

    /// Web's default `Button` with the submit glyph: at rest, then pending
    /// (the icon slot spins, the label reads the progressive word).
    private func pendingButton(_ label: String, loading: Bool) -> some View {
        GlassSubmitLabel(
            label, loading: loading, pendingWord: FeedbackStates.Pending.start, compact: true
        ) {
            AppIcon(AppIcons.uiSubmit, size: AppIcon.Size.small)
        }
        .fixedSize()
    }

    private func card<Content: View>(
        _ index: Int, _ title: String, @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("\(index) · \(title)")
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
