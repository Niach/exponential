import ExpCore
import ExpUI
import SwiftUI

/// EXP-895 — the ONE unified-diff BODY, ×4. Everything about what a diff IS
/// (parsing, counts, the skipped-context arithmetic, every label) lives in
/// ExpCore `Diff` / `DiffPresentation`; this file is presentation only:
/// a `Diff.File`'s hunks as rows of `old · new · sign · text`, painted from
/// `DesignTokens.Diff.*` (never a raw `.green`/`.red`), with the `@@` header on
/// its own band and a plain `N unchanged lines` divider wherever the patch
/// skipped context.
///
/// Unified layout only — a split view on a phone is two unreadable columns.
/// `compact` (a transcript's tool card) drops the OLD gutter and tightens the
/// type; it never changes WHAT is drawn.
///
/// The dividers are plain rows, never buttons: the skipped context is not on
/// the wire, so there is nothing to expand to.
struct DiffPatchBlock: View {
    let file: Diff.File
    var compact = false

    /// Wide enough to dissolve a few characters, narrow enough to leave the
    /// line readable — the horizontal twin of `StickyHeaderFade.height`.
    static let trailingFadeWidth: CGFloat = 36

    /// The layout cap. A 20k-line file must not be handed to SwiftUI as 20k
    /// rows; past this the block says so and stops. (A publisher's OWN cut is a
    /// different fact, reported by `Diff.Parsed.truncatedLines` above this.)
    static let maxRows = 600

    @Environment(\.motion) private var motion
    @State private var viewportWidth: CGFloat = 0
    /// The content's trailing edge in the scroller's coordinate space — it
    /// shrinks as the reader pans right. Seeded past any real width so the
    /// FIRST frame already fades: measured at 0 the block would render crisp
    /// and then flash into the fade one geometry pass later.
    @State private var contentTrailingEdge: CGFloat = .greatestFiniteMagnitude

    private var overflowsTrailing: Bool {
        contentTrailingEdge > viewportWidth + 1
    }

    // MARK: - Rows

    /// One display row. A `gap` is the divider, a `hunk` the verbatim `@@` line,
    /// a `line` one of the file's own rows.
    private enum Row: Identifiable {
        case gap(id: String, text: String)
        case hunk(id: String, text: String)
        case line(id: String, Diff.Line)

        var id: String {
            switch self {
            case let .gap(id, _): id
            case let .hunk(id, _): id
            case let .line(id, _): id
            }
        }
    }

    /// The file flattened: a divider before each hunk that skipped context (the
    /// FIRST hunk measures against the top of the file), the header, the lines.
    private static func rows(of file: Diff.File) -> [Row] {
        var out: [Row] = []
        for (index, hunk) in file.hunks.enumerated() {
            let skipped = index == 0
                ? Diff.unchangedBefore(hunk)
                : Diff.unchangedBetween(file.hunks[index - 1], hunk)
            if skipped > 0 {
                out.append(.gap(id: "g\(index)", text: Diff.unchangedLabel(skipped)))
            }
            out.append(.hunk(id: "h\(index)", text: hunk.header))
            for (at, line) in hunk.lines.enumerated() {
                out.append(.line(id: "l\(index)-\(at)", line))
            }
        }
        return out
    }

    // MARK: - Metrics
    //
    // The font is measured, not guessed: the row washes have to span the whole
    // block (a wash that stops at the end of the text reads as a second cut),
    // and a monospaced advance is the only way to give every row the same width
    // without padding the text with spaces.

    private struct Metrics {
        let font: Font
        let advance: CGFloat
        let gutter: CGFloat
        let sign: CGFloat
        let rowHeight: CGFloat
    }

    private var metrics: Metrics {
        // Dynamic type still drives the size — a transcript card just sits one
        // point below the page's own diff.
        let size = UIFont.preferredFont(forTextStyle: .caption2).pointSize - (compact ? 1 : 0)
        let uiFont = UIFont.monospacedSystemFont(ofSize: size, weight: .regular)
        let advance = ("0" as NSString).size(withAttributes: [.font: uiFont]).width
        return Metrics(
            font: Font(uiFont),
            advance: advance,
            // Four digits plus breathing room — the line numbers are
            // right-aligned inside it (web's `text-right tabular-nums`).
            gutter: advance * 4 + 8,
            sign: advance * 1.5,
            rowHeight: uiFont.lineHeight + 2
        )
    }

    /// The widest row in characters — the block's content width.
    private func contentColumns(_ rows: [Row]) -> Int {
        var widest = 0
        for row in rows {
            let count: Int
            switch row {
            case let .gap(_, text): count = text.count
            case let .hunk(_, text): count = text.count
            case let .line(_, line): count = line.text.count
            }
            if count > widest { widest = count }
        }
        return widest
    }

    var body: some View {
        let all = Self.rows(of: file)
        let shown = Array(all.prefix(Self.maxRows))
        let m = metrics
        let gutters = (compact ? m.gutter : m.gutter * 2) + m.sign
        let textWidth = max(CGFloat(contentColumns(shown)) * m.advance, 1)
        let rowWidth = max(gutters + textWidth, viewportWidth)

        VStack(alignment: .leading, spacing: 0) {
            if file.hunks.isEmpty {
                Text(DiffPresentation.noHunksNote(file))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
            } else {
                ScrollView(.horizontal, showsIndicators: false) {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(shown) { row in
                            rowView(row, m: m, width: rowWidth, gutters: gutters)
                        }
                    }
                    .textSelection(.enabled)
                    .onGeometryChange(for: CGFloat.self, of: { $0.frame(in: .scrollView).maxX }) { edge in
                        contentTrailingEdge = edge
                    }
                }
                .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { width in
                    viewportWidth = width
                }
                .mask(trailingFadeMask)
                if all.count > shown.count {
                    Text("Diff truncated. Showing the first \(shown.count) lines.")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.horizontal, 8)
                        .padding(.vertical, 6)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func rowView(_ row: Row, m: Metrics, width: CGFloat, gutters: CGFloat) -> some View {
        switch row {
        case let .gap(_, text):
            // A PLAIN row, never a button (EXP-895).
            Text(text)
                .font(m.font)
                .foregroundStyle(DesignTokens.Diff.gutterFg)
                .frame(width: width, alignment: .center)
                .background(DesignTokens.Diff.hunkBg.opacity(0.6))
                .accessibilityIdentifier("diff-gap-row")
        case let .hunk(_, text):
            Text(text)
                .font(m.font)
                .foregroundStyle(DesignTokens.Diff.hunkFg)
                .lineLimit(1)
                .padding(.horizontal, 8)
                .frame(width: width, alignment: .leading)
                .background(DesignTokens.Diff.hunkBg)
        case let .line(_, line):
            if line.kind == .meta {
                // `\ No newline at end of file` — numbered on neither side.
                Text(line.text)
                    .font(m.font.italic())
                    .foregroundStyle(DesignTokens.Diff.gutterFg)
                    .lineLimit(1)
                    .padding(.horizontal, 8)
                    .frame(width: width, alignment: .leading)
            } else {
                HStack(spacing: 0) {
                    if !compact {
                        gutterCell(line.oldNo, m: m)
                    }
                    gutterCell(line.newNo, m: m)
                    Text(sign(line.kind))
                        .font(m.font)
                        .foregroundStyle(foreground(line.kind))
                        .frame(width: m.sign, alignment: .center)
                    Text(line.text.isEmpty ? " " : line.text)
                        .font(m.font)
                        .foregroundStyle(foreground(line.kind))
                        .lineLimit(1)
                        .fixedSize(horizontal: true, vertical: false)
                        .frame(width: max(width - gutters, 1), alignment: .leading)
                }
                .frame(width: width, alignment: .leading)
                .background(background(line.kind))
            }
        }
    }

    private func gutterCell(_ number: Int?, m: Metrics) -> some View {
        Text(number.map(String.init) ?? "")
            .font(m.font)
            .monospacedDigit()
            .foregroundStyle(DesignTokens.Diff.gutterFg)
            .lineLimit(1)
            .padding(.trailing, 4)
            .frame(width: m.gutter, alignment: .trailing)
    }

    private func sign(_ kind: Diff.LineKind) -> String {
        switch kind {
        case .add: "+"
        // U+2212 MINUS SIGN, the ×4 rule (`Diff.deletionsLabel`).
        case .del: "\u{2212}"
        default: " "
        }
    }

    private func foreground(_ kind: Diff.LineKind) -> Color {
        switch kind {
        case .add: DesignTokens.Diff.addFg
        case .del: DesignTokens.Diff.delFg
        case .context, .meta: .white.opacity(TextOpacity.secondary)
        }
    }

    private func background(_ kind: Diff.LineKind) -> Color {
        switch kind {
        case .add: DesignTokens.Diff.addBg
        case .del: DesignTokens.Diff.delBg
        case .context, .meta: .clear
        }
    }

    /// Opaque everywhere but the trailing strip, which fades to clear only
    /// while there is more to scroll to. Routed through the motion environment
    /// so Reduce Motion snaps the edge instead of dissolving it.
    private var trailingFadeMask: some View {
        HStack(spacing: 0) {
            Rectangle().fill(.black)
            LinearGradient(
                colors: [.black, .black.opacity(overflowsTrailing ? 0 : 1)],
                startPoint: .leading,
                endPoint: .trailing
            )
            .frame(width: Self.trailingFadeWidth)
        }
        .animation(motion.fast, value: overflowsTrailing)
    }
}
