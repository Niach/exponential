import ExpCore
import ExpUI
import SwiftUI

/// EXP-496: the widget/agent submission metadata card — mobile sibling of
/// web's `widget-submission-card.tsx` (Reporter · Page · Display · User agent
/// · Custom data). Expandable, DEFAULT COLLAPSED, styled like the PR row
/// (rounded `.glassRow()`); the caller renders it only when a submission row
/// exists.
struct WidgetSubmissionCard: View {
    let submission: WidgetSubmissionRow
    let source: String?

    @State private var expanded = false

    private var isAgent: Bool { source == DomainContract.issueSourceAgent }

    private var reporter: String {
        switch (submission.reporterName, submission.reporterEmail) {
        case let (.some(name), .some(email)): return "\(name) <\(email)>"
        case let (.some(name), .none): return name
        case let (.none, .some(email)): return email
        case (.none, .none): return "Anonymous"
        }
    }

    private var display: String {
        var parts: [String] = []
        if let width = submission.viewportWidth, let height = submission.viewportHeight {
            // Locale-independent: "2" for whole ratios, "1.5" otherwise (web parity).
            let dpr = submission.devicePixelRatio.map { ratio in
                ratio == ratio.rounded() ? " @\(Int(ratio))x" : " @\(ratio)x"
            } ?? ""
            parts.append("Viewport \(width)×\(height)\(dpr)")
        }
        if let width = submission.screenWidth, let height = submission.screenHeight {
            parts.append("Screen \(width)×\(height)")
        }
        return parts.joined(separator: " · ")
    }

    private var customDataJson: String? {
        guard let data = submission.customData, !data.isEmpty else { return nil }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        guard let encoded = try? encoder.encode(data) else { return nil }
        return String(data: encoded, encoding: .utf8)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(.easeInOut(duration: 0.2)) { expanded.toggle() }
            } label: {
                HStack(spacing: 8) {
                    AppIcon(isAgent ? AppIcons.uiAgentSource : AppIcons.uiWidget, size: 12)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Text(isAgent ? "Reported by agent" : "Reported via widget")
                        .font(.callout.weight(.medium))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    AppIcon(expanded ? AppIcons.uiChevronUp : AppIcons.uiChevronDown, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(expanded ? "Collapse report details" : "Expand report details")

            if expanded {
                VStack(alignment: .leading, spacing: 6) {
                    row("Reporter", reporter)
                    if let pageUrl = submission.pageUrl {
                        row("Page", pageUrl)
                    }
                    if !display.isEmpty {
                        row("Display", display)
                    }
                    if let userAgent = submission.userAgent {
                        row("User agent", userAgent)
                    }
                    if let json = customDataJson {
                        HStack(alignment: .top, spacing: 8) {
                            rowLabel("Custom data")
                            Text(json)
                                .font(.caption.monospaced())
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(8)
                                // A code block, not a chip: it keeps the
                                // roomier 8pt padding, on the token fill.
                                .background(
                                    GlassTokens.fillSection,
                                    in: RoundedRectangle(cornerRadius: 8)
                                )
                        }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.bottom, 10)
            }
        }
        .glassRow()
    }

    private func row(_ label: String, _ value: String) -> some View {
        HStack(alignment: .top, spacing: 8) {
            rowLabel(label)
            Text(value)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func rowLabel(_ label: String) -> some View {
        Text(label)
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .frame(width: 80, alignment: .leading)
    }
}
