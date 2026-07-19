import ExpUI
import SwiftUI

/// Full-width glass-pill segmented control — the My Work Inbox/My Issues tab
/// language (EXP-192): one `.ultraThinMaterial` capsule container holding
/// equal-width segments, the active one filled white-0.12. Optional
/// per-segment count badge (indigo capsule, the Inbox unread count).
struct GlassSegmentedControl<Option: Hashable>: View {
    let options: [Option]
    let selection: Option
    let label: (Option) -> String
    let badge: (Option) -> Int
    let onSelect: (Option) -> Void

    init(
        options: [Option],
        selection: Option,
        label: @escaping (Option) -> String,
        badge: @escaping (Option) -> Int = { _ in 0 },
        onSelect: @escaping (Option) -> Void
    ) {
        self.options = options
        self.selection = selection
        self.label = label
        self.badge = badge
        self.onSelect = onSelect
    }

    var body: some View {
        HStack(spacing: 4) {
            ForEach(options, id: \.self) { option in
                segmentButton(option)
            }
        }
        .padding(4)
        .background(.ultraThinMaterial, in: Capsule())
        .overlay(
            Capsule().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
        )
    }

    private func segmentButton(_ option: Option) -> some View {
        let active = option == selection
        return Button {
            onSelect(option)
        } label: {
            HStack(spacing: 6) {
                Text(label(option))
                    .font(.subheadline.weight(active ? .semibold : .regular))
                    .foregroundStyle(.white.opacity(active ? 1 : TextOpacity.secondary))
                let count = badge(option)
                if count > 0 {
                    Text("\(count)")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Accent.indigo, in: Capsule())
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 7)
            .background(active ? Color.white.opacity(0.12) : .clear, in: Capsule())
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label(option))
    }
}
