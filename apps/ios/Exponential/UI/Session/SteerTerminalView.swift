import ExpUI
import ExpCore
import SwiftUI

/// Full-screen live terminal mirror of a desktop coding session (masterplan
/// §5c). Watch is primary: relay PTY frames render into the `VTScreen` grid.
/// When the ticket carries the `steer` perm, a claim toggle + input bar let
/// the user type into the remote terminal (single-steerer rule enforced by
/// the relay). A compact presence bar shows watchers + the current steerer.
struct SteerTerminalView: View {
    let accountId: String
    let session: CodingSessionEntity

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @State private var model: SteerViewerModel?
    @State private var inputText = ""
    @FocusState private var inputFocused: Bool

    var body: some View {
        ZStack {
            Color(red: 0.035, green: 0.035, blue: 0.043) // zinc-950 terminal bed
                .ignoresSafeArea()

            VStack(spacing: 0) {
                header
                if let model {
                    presenceBar(model)
                    terminalBody(model)
                    if model.canSteer {
                        steerControls(model)
                    }
                } else {
                    Spacer()
                }
            }
        }
        .onAppear {
            if model == nil {
                let m = SteerViewerModel(
                    accountId: accountId,
                    codingSessionId: session.id,
                    currentUserId: deps.auth.userId,
                    steerApi: deps.steerApi
                )
                model = m
                m.connect()
            }
        }
        .onDisappear {
            model?.disconnect()
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: 8) {
            HStack(spacing: 6) {
                Circle()
                    .fill(headerDotColor)
                    .frame(width: 8, height: 8)
                Text(headerTitle)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
            }
            Spacer()
            if case .closed = model?.phase {
                Button {
                    model?.connect()
                } label: {
                    Label("Reconnect", systemImage: "arrow.clockwise")
                        .font(.caption.weight(.medium))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                }
                .glassButton()
                .buttonStyle(.plain)
                .foregroundStyle(.white)
            }
            Button {
                dismiss()
            } label: {
                Image(systemName: "xmark")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .padding(8)
            }
            .glassButton()
            .buttonStyle(.plain)
            .accessibilityLabel("Close")
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(.ultraThinMaterial)
    }

    private var headerDotColor: Color {
        switch model?.phase {
        case .live: DesignTokens.Semantic.green
        case .connecting: DesignTokens.Semantic.yellow
        default: DesignTokens.Semantic.neutral
        }
    }

    private var headerTitle: String {
        let device = session.deviceLabel.map { " · \($0)" } ?? ""
        switch model?.phase {
        case .connecting: return "Connecting…"
        case .live: return "Live\(device)"
        case .ended: return "Session ended"
        case .closed: return "Disconnected"
        default: return "Terminal\(device)"
        }
    }

    // MARK: - Presence

    @ViewBuilder
    private func presenceBar(_ model: SteerViewerModel) -> some View {
        if !model.viewers.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    Image(systemName: "eye")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    ForEach(model.viewers) { viewer in
                        HStack(spacing: 3) {
                            if viewer.userId == model.steererId {
                                Image(systemName: "keyboard")
                                    .font(.caption2)
                            }
                            Text(viewer.name)
                                .font(.caption2)
                        }
                        .foregroundStyle(
                            viewer.userId == model.steererId
                                ? .white
                                : .white.opacity(TextOpacity.tertiary)
                        )
                    }
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 5)
            }
            .background(Color.white.opacity(0.04))
        }
    }

    // MARK: - Terminal body

    @ViewBuilder
    private func terminalBody(_ model: SteerViewerModel) -> some View {
        ZStack {
            ScrollView([.vertical, .horizontal]) {
                VTGridView(screen: model.screen)
                    .padding(8)
            }
            .defaultScrollAnchor(.bottom)

            switch model.phase {
            case .connecting:
                ProgressView().tint(.white)
            case let .ended(detail):
                statusOverlay(
                    icon: "checkmark.circle",
                    text: detail ?? "The coding session has ended."
                )
            case let .closed(detail):
                statusOverlay(
                    icon: "wifi.slash",
                    text: detail ?? "Connection lost."
                )
            default:
                EmptyView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func statusOverlay(icon: String, text: String) -> some View {
        VStack(spacing: 8) {
            Image(systemName: icon)
                .font(.title3)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text(text)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)
        }
        .padding(16)
        .glassCard()
        .padding(.horizontal, 32)
    }

    // MARK: - Steer controls (claim + input bar)

    @ViewBuilder
    private func steerControls(_ model: SteerViewerModel) -> some View {
        VStack(spacing: 8) {
            HStack(spacing: 8) {
                if model.isSteering {
                    Button {
                        model.release()
                    } label: {
                        Label("Release steering", systemImage: "keyboard.badge.ellipsis")
                            .font(.caption.weight(.medium))
                            .padding(.horizontal, 12)
                            .padding(.vertical, 7)
                    }
                    .glassButton(isActive: true)
                    .buttonStyle(.plain)
                    .foregroundStyle(.white)
                } else {
                    Button {
                        model.claim()
                    } label: {
                        Label("Take steering", systemImage: "keyboard")
                            .font(.caption.weight(.medium))
                            .padding(.horizontal, 12)
                            .padding(.vertical, 7)
                    }
                    .glassButton()
                    .buttonStyle(.plain)
                    .foregroundStyle(.white)
                    .disabled(model.phase != .live || model.remoteSteererName != nil)
                    .opacity(model.phase == .live && model.remoteSteererName == nil ? 1 : 0.5)
                }

                if let name = model.remoteSteererName {
                    Text("\(name) is steering")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                Spacer()
            }

            if model.isSteering {
                HStack(spacing: 8) {
                    keyButton("esc") { model.sendInput("\u{1B}") }
                    keyButton("^C") { model.sendInput("\u{03}") }
                    keyButton("tab") { model.sendInput("\t") }
                    keyButton("↑") { model.sendInput("\u{1B}[A") }
                    keyButton("↓") { model.sendInput("\u{1B}[B") }

                    TextField("Type into the terminal…", text: $inputText)
                        .textFieldStyle(.plain)
                        .font(.caption.monospaced())
                        .foregroundStyle(.white)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .focused($inputFocused)
                        .onSubmit { submitInput(model) }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 8)
                        .glassButton()

                    Button {
                        submitInput(model)
                    } label: {
                        Image(systemName: "return")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.white)
                            .padding(8)
                    }
                    .glassButton()
                    .buttonStyle(.plain)
                    .accessibilityLabel("Send")
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(.ultraThinMaterial)
    }

    private func keyButton(_ label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label)
                .font(.caption2.monospaced().weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .padding(.horizontal, 8)
                .padding(.vertical, 8)
        }
        .glassButton()
        .buttonStyle(.plain)
    }

    private func submitInput(_ model: SteerViewerModel) {
        // Send the line and the Return as SEPARATE input frames. Bundled into
        // one frame they reach the PTY as a single write, which TUI apps
        // (Claude) treat as a paste — the trailing \r inserts instead of
        // submitting, forcing a second Enter press.
        if !inputText.isEmpty {
            model.sendInput(inputText)
        }
        model.sendInput("\r")
        inputText = ""
    }
}

// MARK: - Grid renderer

/// Renders the `VTScreen` cell grid as one monospaced attributed line per row.
/// Attribute runs are coalesced so a mostly-monochrome frame stays cheap.
private struct VTGridView: View {
    let screen: VTScreen

    private static let fontSize: CGFloat = 11

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(0..<screen.rows, id: \.self) { row in
                Text(attributedLine(row))
                    .font(.system(size: Self.fontSize, design: .monospaced))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
            }
        }
        .drawingGroup() // rasterize — dozens of styled Texts per frame otherwise
    }

    private func attributedLine(_ row: Int) -> AttributedString {
        let cells = screen.cells(atRow: row)
        var line = AttributedString()
        guard !cells.isEmpty else { return line }

        let cursorHere = screen.cursorVisible && screen.cursorRow == row
        var runText = ""
        var runStyle: CellStyle? = nil

        func flush() {
            guard let style = runStyle, !runText.isEmpty else { return }
            var piece = AttributedString(runText)
            piece.foregroundColor = style.fg
            if let bg = style.bg { piece.backgroundColor = bg }
            if style.bold { piece.inlinePresentationIntent = .stronglyEmphasized }
            if style.italic {
                piece.inlinePresentationIntent =
                    style.bold ? [.stronglyEmphasized, .emphasized] : .emphasized
            }
            if style.underline { piece.underlineStyle = .single }
            line += piece
            runText = ""
        }

        for (col, cell) in cells.enumerated() {
            let isCursor = cursorHere && screen.cursorCol == col
            let style = CellStyle(cell: cell, isCursor: isCursor)
            if style != runStyle {
                flush()
                runStyle = style
            }
            runText.append(cell.ch)
        }
        flush()
        return line
    }
}

/// Resolved per-cell presentation, comparable so equal-styled cells coalesce.
private struct CellStyle: Equatable {
    let fg: Color
    let bg: Color?
    let bold: Bool
    let italic: Bool
    let underline: Bool

    init(cell: VTCell, isCursor: Bool) {
        var fg = VTPalette.color(cell.fg, isForeground: true)
        var bg = cell.bg == .standard ? nil : VTPalette.color(cell.bg, isForeground: false)
        if cell.inverse != isCursor { // inverse XOR cursor-block
            let newBg = fg
            fg = bg ?? VTPalette.defaultBackground
            bg = newBg
        }
        if cell.dim { fg = fg.opacity(0.6) }
        self.fg = fg
        self.bg = bg
        self.bold = cell.bold
        self.italic = cell.italic
        self.underline = cell.underline
    }
}

/// xterm 256-color palette mapping for `VTColor`.
private enum VTPalette {
    static let defaultForeground = Color(red: 0.894, green: 0.894, blue: 0.905) // zinc-200
    static let defaultBackground = Color(red: 0.035, green: 0.035, blue: 0.043) // zinc-950

    // The classic 16 ANSI colors (xterm defaults, slightly lifted for dark UI).
    private static let ansi16: [(Double, Double, Double)] = [
        (0.10, 0.10, 0.11), (0.80, 0.25, 0.25), (0.30, 0.72, 0.35), (0.80, 0.65, 0.25),
        (0.30, 0.50, 0.90), (0.70, 0.40, 0.80), (0.25, 0.72, 0.72), (0.83, 0.83, 0.85),
        (0.45, 0.45, 0.48), (0.95, 0.40, 0.40), (0.45, 0.85, 0.50), (0.93, 0.82, 0.40),
        (0.45, 0.65, 0.98), (0.85, 0.55, 0.95), (0.40, 0.87, 0.87), (0.98, 0.98, 0.99),
    ]

    static func color(_ vt: VTColor, isForeground: Bool) -> Color {
        switch vt {
        case .standard:
            return isForeground ? defaultForeground : defaultBackground
        case let .palette(index):
            return palette256(Int(index))
        case let .rgb(r, g, b):
            return Color(
                red: Double(r) / 255.0,
                green: Double(g) / 255.0,
                blue: Double(b) / 255.0
            )
        }
    }

    private static func palette256(_ index: Int) -> Color {
        if index < 16 {
            let c = ansi16[index]
            return Color(red: c.0, green: c.1, blue: c.2)
        }
        if index < 232 {
            // 6×6×6 color cube; xterm levels 0,95,135,175,215,255.
            let i = index - 16
            let levels: [Double] = [0, 95, 135, 175, 215, 255]
            let r = levels[(i / 36) % 6] / 255.0
            let g = levels[(i / 6) % 6] / 255.0
            let b = levels[i % 6] / 255.0
            return Color(red: r, green: g, blue: b)
        }
        // Grayscale ramp 232–255: 8 + 10n.
        let v = Double(8 + (index - 232) * 10) / 255.0
        return Color(red: v, green: v, blue: v)
    }
}
