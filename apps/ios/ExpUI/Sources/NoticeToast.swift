import SwiftUI

// EXP-1051: the transient notice capsule, hoisted out of the issue list so any
// surface can answer a tap that cannot do what it asked for.
//
// The pattern it generalises (`IssueListView`'s `noticeCapsule` + its sleeping
// task): one short sentence on an opaque glass card, red when it reports a
// failure, gone on its own a couple of seconds later. Where it replaces a
// permanently printed sentence — the Usage sheet's account refusals — the
// point is that the reason is READ WHERE THE TAP HAPPENED instead of standing
// under every row forever.

/// One transient sentence. Sized by its text; the caller places it.
public struct NoticeToast: View {
    let message: String
    var isError: Bool

    public init(message: String, isError: Bool = false) {
        self.message = message
        self.isError = isError
    }

    public var body: some View {
        Text(message)
            .font(.caption)
            .foregroundStyle(
                isError ? DesignTokens.Semantic.red : .white.opacity(TextOpacity.secondary)
            )
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .glassCard(cornerRadius: 14, isOpaque: true)
            .transition(.move(edge: .bottom).combined(with: .opacity))
            .accessibilityAddTraits(.isStaticText)
    }
}

public extension View {
    /// Float a `NoticeToast` over this view while `message` is set, and clear
    /// the binding after `seconds`. Setting a NEW message restarts the clock
    /// (the auto-dismiss is keyed on the text).
    func noticeToast(
        _ message: Binding<String?>,
        isError: Bool = false,
        seconds: Double = 2.5,
        alignment: Alignment = .bottom
    ) -> some View {
        modifier(
            NoticeToastModifier(
                message: message, isError: isError, seconds: seconds, alignment: alignment
            )
        )
    }
}

public struct NoticeToastModifier: ViewModifier {
    @Binding var message: String?
    let isError: Bool
    let seconds: Double
    let alignment: Alignment

    public func body(content: Content) -> some View {
        content
            .overlay(alignment: alignment) {
                if let text = message {
                    NoticeToast(message: text, isError: isError)
                        .padding(8)
                        .task(id: text) {
                            try? await Task.sleep(for: .seconds(seconds))
                            guard !Task.isCancelled else { return }
                            withAnimation(.easeInOut(duration: 0.18)) { message = nil }
                        }
                }
            }
            .animation(.easeInOut(duration: 0.18), value: message)
    }
}
