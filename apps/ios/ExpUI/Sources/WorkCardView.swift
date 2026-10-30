import ExpCore
import SwiftUI

// EXP-1240 (EXP-1259 D12): THE Work card on iOS — ONE card for a claude
// workflow, a task list and a subagent, replacing the workflow card, the
// task-list block and the subagent group row. The model is ExpCore's
// (`WorkCards.card`, fixture `work-card.json` ×4); this only paints it per the
// fixture's `$comment`: a header (kind icon, name, kind word, progress ·
// elapsed, a spinner while running), a segment bar (phases or tasks), then
// hairline-divided rows. No report text, no token counts, no JSON. A lane row
// (and a subagent card) opens that lane's transcript through the host's
// callbacks; a task row does nothing. Web `@exp/ui` `WorkCard`, desktop
// `ui::work_card`, Android `WorkCard.kt`.
public struct WorkCardView<Footer: View>: View {
    let card: WorkCard
    /// The phases behind a folded row, for its in-place expansion; nil = the
    /// folded row carries no chevron.
    let foldedPhases: [WorkCardFoldedPhase]?
    /// A workflow lane row was picked: `phase` = the phase POSITION (segment
    /// order, a folded phase counts as its own), `lane` = its position inside.
    let onOpenLane: ((_ phase: Int, _ lane: Int) -> Void)?
    /// A subagent card was picked: open its transcript.
    let onOpenSubagent: (() -> Void)?
    /// Rows the host appends under the card's own (an amber duplicate
    /// warning); never report text.
    let footer: Footer

    @State private var foldedOpen = false
    @Environment(\.motion) private var motion

    private typealias G = WorkCards.Geometry

    public init(
        card: WorkCard,
        foldedPhases: [WorkCardFoldedPhase]? = nil,
        onOpenLane: ((_ phase: Int, _ lane: Int) -> Void)? = nil,
        onOpenSubagent: (() -> Void)? = nil,
        @ViewBuilder footer: () -> Footer
    ) {
        self.card = card
        self.foldedPhases = foldedPhases
        self.onOpenLane = onOpenLane
        self.onOpenSubagent = onOpenSubagent
        self.footer = footer()
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if card.kind == .subagent, let onOpenSubagent {
                // The whole subagent card opens its transcript.
                Button(action: onOpenSubagent) {
                    VStack(alignment: .leading, spacing: 0) {
                        header
                        ForEach(Array(card.rows.enumerated()), id: \.offset) { _, row in
                            if case let .status(label) = row { statusRow(label) }
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(PressableRowStyle())
            } else {
                header
                if !card.segments.isEmpty { segmentBar }
                if !card.rows.isEmpty {
                    rows
                }
            }
            footer
        }
        .font(.caption)
        .foregroundStyle(.white)
        .background(GlassTokens.opaqueCardFill)
        .clipShape(RoundedRectangle(cornerRadius: GlassTokens.groupRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.groupRadius)
                .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(WorkCards.TestIds.card)
    }

    // MARK: - Header + bar

    private var kindIcon: String {
        switch card.kind {
        case .workflow: AppIcons.sessionTree
        case .tasks: AppIcons.uiChecklist
        case .subagent: AppIcons.codingSubagent
        }
    }

    private var trailing: String {
        [card.progress, card.elapsed].compactMap { $0 }.joined(separator: WorkCards.separator)
    }

    private var header: some View {
        WorkCardHeaderRow(
            icon: kindIcon,
            name: card.name,
            kindWord: card.kindWord == card.name ? nil : card.kindWord,
            trailing: trailing.isEmpty ? nil : trailing,
            running: card.running
        )
    }

    private var segmentBar: some View {
        HStack(spacing: G.segmentGap) {
            ForEach(Array(card.segments.enumerated()), id: \.offset) { _, segment in
                GeometryReader { proxy in
                    ZStack(alignment: .leading) {
                        Capsule().fill(GlassTokens.fillActive)
                        Capsule()
                            .fill(segment.state == .failed ? DesignTokens.Semantic.red : Color.white)
                            .frame(width: proxy.size.width * CGFloat(min(max(segment.fill, 0), 1)))
                    }
                }
                .frame(height: G.segmentHeight)
            }
        }
        .padding(.horizontal, 12)
        .padding(.bottom, 10)
        .accessibilityHidden(true)
    }

    // MARK: - Rows

    private var rows: some View {
        let positions = card.rowPhases
        return VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(card.rows.enumerated()), id: \.offset) { index, row in
                GlassDivider()
                switch row {
                case let .folded(label, lanes, duration):
                    foldedRow(label: label, lanes: lanes, duration: duration)
                case let .phase(title, state, detail, duration, phaseLanes):
                    phaseRow(
                        title: title, state: state, detail: detail, duration: duration,
                        lanes: phaseLanes, phase: positions.indices.contains(index) ? positions[index] : index
                    )
                case let .task(label, state):
                    HStack(spacing: 8) {
                        stateGlyph(state)
                        Text(label).lineLimit(1).truncationMode(.tail)
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 12)
                    .frame(height: G.row)
                case let .status(label):
                    statusRow(label)
                }
            }
        }
    }

    private func statusRow(_ label: String) -> some View {
        HStack {
            Text(label).foregroundStyle(muted).lineLimit(1)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .frame(height: G.row)
    }

    @ViewBuilder
    private func foldedRow(label: String, lanes: String, duration: String?) -> some View {
        let expandable = !(foldedPhases ?? []).isEmpty
        let head = HStack(spacing: 8) {
            AppIcon(AppIcons.uiCheck, size: 12).foregroundStyle(muted)
            Text(label).lineLimit(1).truncationMode(.tail)
            Text(lanes).foregroundStyle(muted).lineLimit(1).fixedSize()
            Spacer(minLength: 8)
            if let duration {
                Text(duration).monospacedDigit().foregroundStyle(muted)
            }
            if expandable {
                AppIcon(AppIcons.uiChevronRight, size: 12)
                    .foregroundStyle(muted)
                    .rotationEffect(.degrees(foldedOpen ? 90 : 0))
            }
        }
        .padding(.horizontal, 12)
        .frame(height: G.row)
        .contentShape(Rectangle())
        VStack(alignment: .leading, spacing: 0) {
            if expandable {
                Button {
                    withAnimation(motion.standard) { foldedOpen.toggle() }
                } label: { head }
                    .buttonStyle(PressableRowStyle())
                    .accessibilityAddTraits(foldedOpen ? .isSelected : [])
            } else {
                head
            }
            if foldedOpen, let foldedPhases {
                ForEach(Array(foldedPhases.enumerated()), id: \.offset) { phaseIndex, phase in
                    HStack(spacing: 8) {
                        Text(phase.title).foregroundStyle(muted).lineLimit(1)
                        Spacer(minLength: 8)
                        if let duration = phase.duration {
                            Text(duration).monospacedDigit().foregroundStyle(muted)
                        }
                    }
                    .padding(.leading, G.laneIndent)
                    .padding(.trailing, 12)
                    .frame(height: G.lane)
                    ForEach(Array(phase.lanes.enumerated()), id: \.offset) { laneIndex, lane in
                        laneRow(lane, phase: phaseIndex, lane: laneIndex)
                    }
                }
                .transition(.opacity)
            }
        }
        .accessibilityIdentifier(WorkCards.TestIds.folded)
    }

    private func phaseRow(
        title: String, state: WorkState, detail: String?, duration: String?,
        lanes: [WorkCardLane], phase: Int
    ) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                stateGlyph(state)
                Text(title)
                    .foregroundStyle(state == .pending ? muted : .white)
                    .lineLimit(1)
                    .truncationMode(.tail)
                if let detail {
                    Text(detail).foregroundStyle(muted).lineLimit(1).truncationMode(.tail)
                }
                Spacer(minLength: 8)
                if let duration {
                    Text(duration).monospacedDigit().foregroundStyle(muted)
                }
            }
            .padding(.horizontal, 12)
            .frame(height: G.row)
            ForEach(Array(lanes.enumerated()), id: \.offset) { laneIndex, lane in
                laneRow(lane, phase: phase, lane: laneIndex)
            }
        }
    }

    @ViewBuilder
    private func laneRow(_ lane: WorkCardLane, phase: Int, lane laneIndex: Int) -> some View {
        let content = HStack(spacing: 8) {
            laneDot(lane.state)
            Text(lane.label).lineLimit(1).truncationMode(.tail)
            if let model = lane.model {
                Text(model).foregroundStyle(muted).lineLimit(1).fixedSize()
            }
            Spacer(minLength: 8)
            if lane.state == "running" {
                InlineSpinner(size: 12)
            } else if let duration = lane.duration {
                Text(duration).monospacedDigit().foregroundStyle(muted)
            }
        }
        .foregroundStyle(lane.state == "error" ? DesignTokens.Semantic.red : .white)
        .padding(.leading, G.laneIndent)
        .padding(.trailing, 12)
        .frame(height: G.lane)
        .contentShape(Rectangle())
        if let onOpenLane {
            Button { onOpenLane(phase, laneIndex) } label: { content }
                .buttonStyle(PressableRowStyle())
                .accessibilityIdentifier(WorkCards.TestIds.lane)
        } else {
            content.accessibilityIdentifier(WorkCards.TestIds.lane)
        }
    }

    private func laneDot(_ state: String) -> some View {
        Group {
            switch state {
            case "running": Circle().fill(DesignTokens.Semantic.green)
            case "error": Circle().fill(DesignTokens.Semantic.red)
            case "queued": Circle().strokeBorder(muted, lineWidth: 1)
            default: Circle().fill(muted)
            }
        }
        .frame(width: 6, height: 6)
    }

    @ViewBuilder
    private func stateGlyph(_ state: WorkState) -> some View {
        switch state {
        case .running: InlineSpinner(size: 12)
        case .done: AppIcon(AppIcons.uiCheck, size: 12).foregroundStyle(muted)
        case .failed: AppIcon(AppIcons.uiClose, size: 12).foregroundStyle(DesignTokens.Semantic.red)
        case .pending:
            AppIcon(AppIcons.statusBacklog, size: 12)
                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
        }
    }

    private var muted: Color { .white.opacity(TextOpacity.tertiary) }
}

extension WorkCardView where Footer == EmptyView {
    public init(
        card: WorkCard,
        foldedPhases: [WorkCardFoldedPhase]? = nil,
        onOpenLane: ((_ phase: Int, _ lane: Int) -> Void)? = nil,
        onOpenSubagent: (() -> Void)? = nil
    ) {
        self.init(
            card: card, foldedPhases: foldedPhases, onOpenLane: onOpenLane,
            onOpenSubagent: onOpenSubagent
        ) { EmptyView() }
    }
}

/// THE card's header row (`work-card.json` geometry.header): the icon, the
/// medium name (truncates), the muted kind word, then trailing the muted
/// tabular `trailing` with a 12pt spinner in front while `running`. The Work
/// card draws it at its top; EXP-1295's session card IS this row alone.
public struct WorkCardHeaderRow: View {
    let icon: String
    var iconTint: Color = .white.opacity(TextOpacity.tertiary)
    let name: String
    var kindWord: String?
    var kindTint: Color = .white.opacity(TextOpacity.tertiary)
    var trailing: String?
    var running = false
    /// A chevron at the trailing edge: the row opens something.
    var chevron = false
    /// #EXP-1464: the chevron turned down (the row expanded in place).
    var expanded = false

    public init(
        icon: String,
        iconTint: Color = .white.opacity(TextOpacity.tertiary),
        name: String,
        kindWord: String?,
        kindTint: Color = .white.opacity(TextOpacity.tertiary),
        trailing: String?,
        running: Bool,
        chevron: Bool = false,
        expanded: Bool = false
    ) {
        self.icon = icon
        self.iconTint = iconTint
        self.name = name
        self.kindWord = kindWord
        self.kindTint = kindTint
        self.trailing = trailing
        self.running = running
        self.chevron = chevron
        self.expanded = expanded
    }

    public var body: some View {
        HStack(spacing: 8) {
            AppIcon(icon, size: 13)
                .foregroundStyle(iconTint)
            Text(name)
                .fontWeight(.medium)
                .lineLimit(1)
                .truncationMode(.tail)
            if let kindWord {
                Text(kindWord)
                    .foregroundStyle(kindTint)
                    .lineLimit(1)
                    .fixedSize()
            }
            Spacer(minLength: 8)
            if running || trailing != nil {
                HStack(spacing: 6) {
                    if running { InlineSpinner(size: 12) }
                    if let trailing {
                        Text(trailing)
                            .monospacedDigit()
                            .foregroundStyle(muted)
                            .lineLimit(1)
                            .fixedSize()
                    }
                }
            }
            if chevron {
                AppIcon(AppIcons.uiChevronRight, size: 12)
                    .foregroundStyle(muted)
                    .rotationEffect(.degrees(expanded ? 90 : 0))
            }
        }
        .padding(.horizontal, 12)
        .frame(height: WorkCards.Geometry.header)
    }

    private var muted: Color { .white.opacity(TextOpacity.tertiary) }
}

/// EXP-1295: THE session card (fixtures/session-card.json) — a session inside
/// a thread, and a thread an agent started, as ONE card that is the Work
/// card's header row alone: the state icon in its tone, the name, the muted
/// state word, trailing the elapsed with the spinner while working. Tapping
/// it opens that session. Model = ExpCore `SessionTree.sessionCard`.
public struct SessionCardView: View {
    let card: SessionCard
    let onOpen: (() -> Void)?
    /// EXP-1323: drawn as a row INSIDE another card (a started suggestion):
    /// no shell of its own.
    let bare: Bool
    /// #EXP-1464 (thread-row-action.json): expanded in place (the chevron
    /// turns down); `kindWord` / `tone` override the card's own (`Answered`,
    /// muted), nil = its own.
    let expanded: Bool
    let kindWord: String?
    let tone: String?

    public init(
        card: SessionCard,
        bare: Bool = false,
        expanded: Bool = false,
        kindWord: String? = nil,
        tone: String? = nil,
        onOpen: (() -> Void)?
    ) {
        self.card = card
        self.bare = bare
        self.expanded = expanded
        self.kindWord = kindWord
        self.tone = tone
        self.onOpen = onOpen
    }

    public var body: some View {
        Group {
            if let onOpen {
                Button(action: onOpen) { content.contentShape(Rectangle()) }
                    .buttonStyle(PressableRowStyle())
            } else {
                content
            }
        }
        .font(.caption)
        .foregroundStyle(.white)
        .background(bare ? Color.clear : GlassTokens.opaqueCardFill)
        .clipShape(RoundedRectangle(cornerRadius: bare ? 0 : GlassTokens.groupRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.groupRadius)
                .stroke(bare ? Color.clear : GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(card.name), \(kindWord ?? card.kindWord)")
        .accessibilityIdentifier(card.testId)
    }

    /// The header row.
    private var content: some View { row }

    private var row: some View {
        let tone = tone ?? card.tone
        return WorkCardHeaderRow(
            icon: Self.glyph(card.state),
            iconTint: Self.tint(tone),
            name: card.name,
            kindWord: kindWord ?? card.kindWord,
            // #EXP-1466 session-card.json: the word stays muted unless amber.
            kindTint: tone == "amber"
                ? Self.tint(tone)
                : .white.opacity(TextOpacity.tertiary),
            trailing: card.elapsed,
            running: card.running,
            chevron: onOpen != nil,
            expanded: expanded
        )
    }

    /// The fixture's `icons` concepts.
    static func glyph(_ state: SessionState) -> String {
        switch state {
        case .working: AppIcons.codingRunning
        case .idle: AppIcons.codingAssistant
        case .needsYou: AppIcons.codingNeedsInput
        case .done: AppIcons.codingEnded
        }
    }

    /// The fixture's `tones`: muted | foreground | amber | rose.
    static func tint(_ tone: String) -> Color {
        switch tone {
        case "foreground": .white
        case "amber": DesignTokens.Semantic.yellow
        case "rose": DesignTokens.Semantic.red
        default: .white.opacity(TextOpacity.tertiary)
        }
    }
}

/// A tappable row inside a card: the active fill while pressed (no scale —
/// a row is not a button face).
public struct PressableRowStyle: ButtonStyle {
    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(configuration.isPressed ? GlassTokens.fillActive : Color.clear)
    }
}
