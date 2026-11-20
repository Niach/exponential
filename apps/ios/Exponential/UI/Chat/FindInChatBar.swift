import ExpCore
import ExpUI
import SwiftUI
#if canImport(ExponentialUI)
import ExponentialUI
#endif

private typealias Measures = Chats.Shell.ChatHeader

/// #EDE-130 (find-in-chat.json): find in chat on a phone. The chat screen
/// owns the bar (`ChatFind`: open, the field, the steps it asks for, the
/// counter and status line the transcript reports back); the ROOT's
/// transcript (`AgentSessionView`, `TranscriptFind`) counts its rows,
/// merges the older hits and lands on the current stop; its rows read
/// `FindPaint` for genUI's `highlight`.
@Observable
@MainActor
final class ChatFind {
    struct StepRequest: Equatable {
        let dir: FindInChat.Step
        let n: Int
    }

    struct Report: Equatable {
        var counter = ""
        var status: String?
    }

    /// The last query per chat, for the app's life (memory only).
    private static var lastQuery: [String: String] = [:]

    let chatKey: String
    private(set) var open = false
    /// #EDE-99: the session whose transcript the bar searches (nil = the
    /// root's; a global search pick opens a child's).
    private(set) var sessionId: String?
    /// #EDE-99: a global search pick's start (`globalChatStart`), dropped at
    /// the first step or edit.
    private(set) var target: SearchList.GlobalChatTarget?
    /// The field takes the focus when the bar shows (a pick lands without
    /// the keyboard).
    private(set) var takesFocus = true
    /// The field's text.
    var raw = "" {
        didSet {
            guard raw != oldValue else { return }
            Self.lastQuery[chatKey] = raw
            target = nil
            schedule()
        }
    }
    /// The query the transcript searches (collapsed, debounced; "" = none).
    private(set) var query = ""
    /// The last step the bar asked for (`n` bumps per press).
    private(set) var step: StepRequest?
    private(set) var report = Report()
    /// Bumps when the field should take the focus (open, ⌘F again).
    private(set) var focusNonce = 0
    @ObservationIgnored private var debounce: Task<Void, Never>?

    init(chatKey: String) {
        self.chatKey = chatKey
    }

    func openBar() {
        takesFocus = true
        if !open {
            open = true
            sessionId = nil
            raw = Self.lastQuery[chatKey] ?? ""
            query = FindInChat.query(raw)
        }
        focusNonce &+= 1
    }

    /// #EDE-99: a global search pick: open on `seed.query` at once over its
    /// session, the hit's stop first.
    func seed(_ seed: ChatFindSeed) {
        open = true
        takesFocus = false
        sessionId = seed.sessionId
        raw = seed.query
        debounce?.cancel()
        query = FindInChat.query(seed.query)
        target = seed.target
        step = nil
    }

    /// Clears every mark; the scroll and the opened groups stay.
    func close() {
        debounce?.cancel()
        open = false
        sessionId = nil
        target = nil
        query = ""
        step = nil
        report = Report()
    }

    func go(_ dir: FindInChat.Step) {
        target = nil
        step = StepRequest(dir: dir, n: (step?.n ?? 0) &+ 1)
    }

    func setReport(_ next: Report) {
        if next != report { report = next }
    }

    /// Typing re-searches after `debounceMs`; a blank query at once.
    private func schedule() {
        debounce?.cancel()
        guard open else { return }
        let next = FindInChat.query(raw)
        guard !next.isEmpty else {
            query = ""
            return
        }
        debounce = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(FindInChat.debounceMs))
            guard !Task.isCancelled, let self, self.open else { return }
            if self.query != next { self.query = next }
        }
    }
}

private struct ChatFindKey: EnvironmentKey {
    static let defaultValue: ChatFind? = nil
}

extension EnvironmentValues {
    /// The chat screen's find, for the ROOT's transcript under it.
    var chatFind: ChatFind? {
        get { self[ChatFindKey.self] }
        set { self[ChatFindKey.self] = newValue }
    }
}

/// COMPACT (find-in-chat.json WHERE THE BAR SITS): the bar REPLACES the chat
/// header while open: back (closes), the field, the counter, previous
/// (chevron up), next (chevron down); the status line under it. THE chat
/// header's row, its ghost buttons and the glass field, nothing new.
struct FindInChatHeader: View {
    @Bindable var find: ChatFind
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: CGFloat(Measures.gap)) {
                ChatHeaderGhostButton(icon: AppIcons.uiBack, label: FindInChat.Copy.close, identifier: FindInChat.TestIds.close) {
                    find.close()
                }
                GlassTextField(
                    FindInChat.Copy.placeholder, text: $find.raw, bordered: false,
                    accessibilityIdentifier: FindInChat.TestIds.input
                )
                .focused($focused)
                .submitLabel(.search)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                // Enter = previous (older), the way a chat is read back.
                .onSubmit {
                    find.go(.previous)
                    focused = true
                }
                .onKeyPress(.escape) {
                    find.close()
                    return .handled
                }
                .onKeyPress(.return, phases: .down) { press in
                    guard press.modifiers.contains(.shift) else { return .ignored }
                    find.go(.next)
                    return .handled
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                if !find.report.counter.isEmpty {
                    Text(find.report.counter)
                        .font(.subheadline.monospacedDigit())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                        .fixedSize()
                        .accessibilityIdentifier(FindInChat.TestIds.counter)
                }
                ChatHeaderGhostButton(icon: AppIcons.uiChevronUp, label: FindInChat.Copy.previous, identifier: FindInChat.TestIds.previous) {
                    find.go(.previous)
                }
                ChatHeaderGhostButton(icon: AppIcons.uiChevronDown, label: FindInChat.Copy.next, identifier: FindInChat.TestIds.next) {
                    find.go(.next)
                }
            }
            .padding(.horizontal, CGFloat(Measures.paddingX))
            .frame(maxWidth: .infinity, minHeight: CGFloat(Measures.row), maxHeight: CGFloat(Measures.row), alignment: .leading)
            if let status = find.report.status {
                // The status line: the transcript's first line under the bar.
                Text(status)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Palette.mutedForeground)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16)
                    .padding(.bottom, 8)
                    .accessibilityIdentifier(FindInChat.TestIds.status)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(FindInChat.TestIds.bar)
        .onAppear { if find.takesFocus { focused = true } }
        .onChange(of: find.focusNonce) { _, _ in focused = true }
    }
}

// MARK: - Painting

/// What the transcript's rows paint: the query, the current stop's row and
/// local index, the tool rows whose output matches, the groups find opened
/// and the ringed row (feed item ids).
struct FindPaint: Equatable, Sendable {
    var query: String
    var currentItem: Int?
    var currentLocal = 0
    /// Tool rows whose output matches → its match count (the hint).
    var outputHits: [Int: Int] = [:]
    /// Folded groups find opened (by their first item; they stay open).
    var openGroups: Set<Int> = []
    var ring: Int?

    /// The `highlight` of feed item `id`: the query, the current match when
    /// it is the current stop's row.
    func mark(_ id: Int) -> FindMark {
        FindMark(query: query, current: currentItem == id ? currentLocal : nil)
    }
}

/// genUI's `highlight` prop (text-highlight.json) as the host hands it down.
struct FindMark: Equatable, Sendable {
    let query: String
    let current: Int?
}

private struct FindPaintKey: EnvironmentKey {
    static let defaultValue: FindPaint? = nil
}

private struct FindMarkKey: EnvironmentKey {
    static let defaultValue: FindMark? = nil
}

extension EnvironmentValues {
    /// The transcript's find paint (nil = find is not searching).
    var findPaint: FindPaint? {
        get { self[FindPaintKey.self] }
        set { self[FindPaintKey.self] = newValue }
    }

    /// The marks the read-only Markdown under it paints (`AgentMarkdownText`).
    var findMark: FindMark? {
        get { self[FindMarkKey.self] }
        set { self[FindMarkKey.self] = newValue }
    }
}

/// The highlight rule ×4 (genUI `TextHighlight`): counts, ranges, the
/// theme's two fills.
@MainActor
enum FindMarks {
    static func count(_ text: String, query: String) -> Int {
        #if canImport(ExponentialUI)
        TextHighlight.count(text, query: query)
        #else
        0
        #endif
    }

    /// A bubble's or narration's painted count (`nodeHighlightCount`).
    static func markdownCount(_ text: String, query: String) -> Int {
        #if canImport(ExponentialUI)
        TextHighlight.markdownCount(text, query: query)
        #else
        0
        #endif
    }

    /// A Trace line title's marks (`Row`/`Text` `highlight`).
    static func title(_ title: String, mark: FindMark) -> TraceTitleMarks? {
        #if canImport(ExponentialUI)
        let ranges = TextHighlight.ranges(title, query: mark.query)
        guard !ranges.isEmpty else { return nil }
        let model = AppMarkdownHost.host(nil, deps: nil)?.model
        let match = model?.color("match") ?? DesignTokens.Semantic.yellow.opacity(0.3)
        return TraceTitleMarks(
            ranges: ranges.map(\.nsRange), current: mark.current ?? -1,
            match: match, matchCurrent: model?.color("matchCurrent") ?? match
        )
        #else
        nil
        #endif
    }
}
