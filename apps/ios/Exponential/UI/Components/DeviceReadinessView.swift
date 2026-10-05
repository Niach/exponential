import ExpCore
import ExpUI
import SwiftUI

// EXP-1196/1218/1219: THE device readiness block (contract fixture
// `device-doctor.json`, row model `DeviceReadiness` in ExpCore). Groups are
// filled bands over flat hairline-divided rows: state glyph, label, the
// device-written detail, at most ONE trailing pill. The `computer_use` item
// IS a plain switch row. No subtitles, no footers.
//
// Three forms over the same row view:
//   block   — every group (Device settings).
//   row     — ONE row, the composer's "cannot start here" line.
//   compact — rows without pills (device setup's status line; the row it
//             sits in is the tap target).
struct DeviceReadinessView: View {
    private enum Content {
        case block([DeviceReadiness.Group])
        case rows([DeviceReadiness.Row])
    }

    private let content: Content
    private let showsActions: Bool
    private let computerUse: Binding<Bool>?
    private let busyActions: Set<String>
    private let onAction: (DeviceReadiness.Row) -> Void

    /// The whole block. `computerUse` drives the switch row (nil = read-only
    /// at the report's state); `busyActions` = row keys whose action is in
    /// flight (their pill disables).
    init(
        groups: [DeviceReadiness.Group],
        computerUse: Binding<Bool>? = nil,
        busyActions: Set<String> = [],
        onAction: @escaping (DeviceReadiness.Row) -> Void
    ) {
        content = .block(groups)
        showsActions = true
        self.computerUse = computerUse
        self.busyActions = busyActions
        self.onAction = onAction
    }

    /// One row with its action (the composer).
    init(
        row: DeviceReadiness.Row,
        busy: Bool = false,
        onAction: @escaping (DeviceReadiness.Row) -> Void
    ) {
        content = .rows([row])
        showsActions = true
        computerUse = nil
        busyActions = busy ? [row.key] : []
        self.onAction = onAction
    }

    /// Bare rows, no pills (device setup).
    init(compactRows: [DeviceReadiness.Row]) {
        content = .rows(compactRows)
        showsActions = false
        computerUse = nil
        busyActions = []
        onAction = { _ in }
    }

    var body: some View {
        switch content {
        case .block(let groups):
            VStack(alignment: .leading, spacing: 12) {
                ForEach(groups) { group in
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand(group.label) {
                            if let tag = group.tag {
                                GlassPill(tag)
                            }
                        }
                        rowList(group.rows)
                    }
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("device-readiness-group-\(group.key)")
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("device-readiness")
        case .rows(let rows):
            rowList(rows)
        }
    }

    private func rowList(_ rows: [DeviceReadiness.Row]) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                if index > 0 { GlassDivider() }
                rowView(row)
            }
        }
    }

    @ViewBuilder
    private func rowView(_ row: DeviceReadiness.Row) -> some View {
        Group {
            if row.isSwitch {
                switchRow(row)
            } else {
                statusRow(row)
            }
        }
        .padding(.leading, row.indented ? 28 : 0)
        .padding(.horizontal, showsActions ? 12 : 0)
        .padding(.vertical, showsActions ? 8 : 2)
        .frame(minHeight: showsActions ? 40 : nil)
        .flatRow()
    }

    private func switchRow(_ row: DeviceReadiness.Row) -> some View {
        Toggle(
            row.label,
            isOn: computerUse ?? .constant(row.switchOn)
        )
        .font(.subheadline)
        .disabled(computerUse == nil)
        .accessibilityIdentifier("device-computer-use")
    }

    private func statusRow(_ row: DeviceReadiness.Row) -> some View {
        HStack(spacing: 8) {
            if let glyph = row.glyph {
                AppIcon(Self.icon(glyph), size: showsActions ? AppIcon.Size.small : 11)
                    .foregroundStyle(Self.color(row.tone))
            }
            Text(row.label)
                .font(showsActions ? .subheadline : .caption)
                .foregroundStyle(.white.opacity(showsActions ? TextOpacity.primary : TextOpacity.secondary))
                .lineLimit(1)
            if let detail = row.detail {
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 8)
            if showsActions, let action = row.action, let label = row.actionLabel {
                GlassPill(
                    label,
                    mode: .action { onAction(row) },
                    primary: row.primary,
                    enabled: !busyActions.contains(row.key)
                )
                .accessibilityIdentifier("device-readiness-action-\(action)-\(row.key)")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("device-readiness-row-\(row.key)")
    }

    private static func icon(_ glyph: DeviceReadiness.Glyph) -> String {
        switch glyph {
        case .check: AppIcons.uiCheck
        case .alert: AppIcons.uiWarning
        case .dash: AppIcons.uiMinus
        case .x: AppIcons.uiClose
        }
    }

    private static func color(_ tone: DeviceReadiness.Tone?) -> Color {
        switch tone {
        case .success: DesignTokens.Semantic.green
        case .warning: DesignTokens.Semantic.yellow
        case .destructive: DesignTokens.Semantic.red
        case .muted, nil: .white.opacity(TextOpacity.tertiary)
        }
    }
}
