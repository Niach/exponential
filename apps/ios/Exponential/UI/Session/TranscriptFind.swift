import ExpCore
import Foundation
import Observation
import SwiftUI

/// #EDE-130 (find-in-chat.json): the transcript's half of find in chat on a
/// phone: counts the WHOLE loaded feed (`AgentSessionModel.findFeedRows`,
/// not the painted window), merges the chat store's and the device's older
/// hits, keeps the current stop while answers arrive, says where to land
/// (`focus`: THE deep-link scroll path pages an older row in, centres the
/// row, rings it) and reports the counter and status line to the bar.
@Observable
@MainActor
final class TranscriptFind {
    /// A loaded row with what landing and painting need.
    struct Row: Equatable {
        let row: FindInChat.Row
        let itemId: Int
        /// Its output's matches (the hint), 0 = none.
        let outputCount: Int
    }

    private struct Older: Equatable {
        var query = ""
        var store: [FindInChat.OlderHit] = []
        var device: [FindInChat.OlderHit] = []
        var storePhase: FindInChat.Phase = .none
        var devicePhase: FindInChat.Phase = .none
    }

    private(set) var query = ""
    private(set) var rows: [Row] = []
    private(set) var stops: [FindInChat.Stop] = []
    private var older = Older()
    private var current: FindInChat.Stop?
    private var lastStep = 0
    /// The landing in flight: THE deep link's target (the stop's key) and
    /// whether it rings.
    private(set) var focus: WaitingFocus?
    private(set) var rings = false
    /// Items inside folded groups find opened (they stay open).
    private(set) var openItems: Set<Int> = []
    /// The ringed row's key while its ring shows (`DeepLinkScroll`).
    var ringKey: String?

    private var lowestSeq: Int?
    @ObservationIgnored private var searches: [Task<Void, Never>] = []

    /// Find is searching: the follow pin holds (the reader stays on the stop).
    var holding: Bool { !query.isEmpty && !stops.isEmpty }

    private var index: Int? {
        guard !stops.isEmpty else { return nil }
        if let current, let at = stops.firstIndex(where: { $0.same(current) }) { return at }
        return stops.count - 1
    }

    // MARK: - Inputs

    /// A new query (or none): drop the old answers, ask the store and, while
    /// the loaded feed does not reach the first event and the device is
    /// online, the device; start at the newest stop.
    func search(_ next: String, model: AgentSessionModel, accountId: String, rootId: String, sessionId: String) {
        guard next != query else { return }
        for task in searches { task.cancel() }
        searches = []
        query = next
        current = nil
        focus = nil
        ringKey = nil
        guard !next.isEmpty else {
            older = Older()
            rows = []
            stops = []
            return
        }
        let fromStart = model.findFromStart
        let online = model.hostDevice.online
        let askDevice = !fromStart && online == true
        older = Older(
            query: next, storePhase: .searching,
            devicePhase: fromStart ? .none : askDevice ? .searching : online == false ? .offline : .none
        )
        recount(model: model, sessionId: sessionId)
        searches.append(Task { [weak self] in
            let json = await Task.detached(priority: .userInitiated) {
                ChatStoreHub.shared.searchChat(accountId: accountId, rootId: rootId, query: next, limit: FindInChat.hitsMax)
            }.value
            guard let self, !Task.isCancelled, self.older.query == next else { return }
            self.older.store = FindInChat.storeHits(json, sessionId: sessionId)
            self.older.storePhase = .done
            self.restop()
        })
        if askDevice {
            let before = model.findLowestSeq
            searches.append(Task { [weak self, weak model] in
                let result = await model?.search(query: next, beforeSeq: before)
                guard let self, !Task.isCancelled, self.older.query == next else { return }
                self.older.device = FindInChat.deviceHits((result?.hits ?? []).map { (seq: $0.seq, kind: $0.kind) }, sessionId: sessionId)
                self.older.devicePhase = .done
                self.restop()
            })
        }
    }

    /// The feed moved (a streamed row, a page paged in): count it again,
    /// keeping the current stop.
    func recount(model: AgentSessionModel, sessionId: String) {
        guard !query.isEmpty else { return }
        let next = Self.rows(model, sessionId: sessionId, query: query)
        lowestSeq = model.findLowestSeq
        if next != rows { rows = next }
        restop()
    }

    /// The bar asked for a step (`n` bumps per press).
    func step(_ request: ChatFind.StepRequest?) {
        guard let request, request.n != lastStep else { return }
        lastStep = request.n
        guard let next = FindInChat.step(current: index, total: stops.count, request.dir) else { return }
        land(stops[next])
    }

    private func restop() {
        let hits = FindInChat.below(older.store + older.device, lowestSeq: lowestSeq)
        let next = FindInChat.stops(rows: rows.map(\.row), older: hits)
        if next != stops { stops = next }
        // A new query starts at the NEWEST stop and lands there.
        if current == nil, let newest = stops.last { land(newest) }
    }

    // MARK: - Landing

    private func land(_ stop: FindInChat.Stop) {
        current = stop
        let row = rows.first { $0.row.key == stop.key }
        let landing = FindInChat.land(row.map { _ in
            FindInChat.Stop(key: stop.key, local: stop.local, loaded: true, group: row?.row.group, output: stop.output)
        } ?? stop)
        if landing.openGroup != nil, let row { openItems.insert(row.itemId) }
        rings = landing.ring
        focus = WaitingFocus(id: stop.key, at: Date())
    }

    /// The feed item a find key names (nil = not loaded).
    func itemId(_ key: String) -> Int? {
        rows.first { $0.row.key == key }?.itemId
    }

    // MARK: - Outputs

    func report() -> ChatFind.Report {
        guard !query.isEmpty else { return ChatFind.Report() }
        let done = older.storePhase != .searching && older.devicePhase != .searching
        return ChatFind.Report(
            counter: FindInChat.counter(current: index, total: stops.count, done: done, compact: true),
            status: FindInChat.status(store: older.storePhase, device: older.devicePhase, deviceName: deviceName)
        )
    }

    var deviceName = "Desktop"

    /// What the rows paint (nil = nothing searched).
    var paint: FindPaint? {
        guard !query.isEmpty else { return nil }
        var paint = FindPaint(query: query)
        if let index {
            let stop = stops[index]
            if stop.loaded, !stop.output, let id = itemId(stop.key) {
                paint.currentItem = id
                paint.currentLocal = stop.local
            }
        }
        for row in rows where row.outputCount > 0 { paint.outputHits[row.itemId] = row.outputCount }
        paint.openGroups = openItems
        if rings, let ringKey { paint.ring = itemId(ringKey) }
        return paint
    }

    // MARK: - The rows (find-in-chat.json WHAT IS SEARCHED)

    /// The loaded transcript's countable rows for `query`, top to bottom:
    /// a narration's and a person's bubble text (as the bubbles draw it), a
    /// plain tool line's title (+ its output's stop), a folded run's members
    /// (`group`); never thoughts, cards or markers.
    static func rows(_ model: AgentSessionModel, sessionId: String, query: String) -> [Row] {
        var out: [Row] = []
        var seen: Set<String> = []
        func key(_ id: Int) -> String {
            let base = model.findSeq(id).map { FindInChat.eventKey(sessionId: sessionId, seq: $0) } ?? "i:\(id)"
            defer { seen.insert(base) }
            return seen.contains(base) ? "\(base)#\(id)" : base
        }
        for (row, checkIns) in model.findFeedRows() {
            switch row {
            case let .toolRun(items):
                let group = "g:\(items.first?.id ?? 0)"
                for item in items {
                    if let found = toolRow(item, key: key(item.id), query: query, group: group, model: model) { out.append(found) }
                }
            case let .single(item):
                switch item {
                case let .narration(id, text, _, _, _):
                    guard !TranscriptGrouping.isBlankText(text) else { continue }
                    out.append(Row(row: FindInChat.Row(key: key(id), count: FindMarks.markdownCount(text, query: query)), itemId: id, outputCount: 0))
                case let .userMessage(id, text, _, _):
                    guard checkIns == nil, let shown = bubbleText(text, model: model) else { continue }
                    let count = shown.isEmpty ? 0 : FindMarks.markdownCount(shown, query: query)
                    out.append(Row(row: FindInChat.Row(key: key(id), count: count), itemId: id, outputCount: 0))
                case let .tool(_, name, _, _, callId, _, _, _, diff, _, _, _, _):
                    guard TranscriptGrouping.traceNotice(name) == nil, diff?.isEmpty != false,
                          model.workflow(for: callId) == nil,
                          let found = toolRow(item, key: key(item.id), query: query, group: nil, model: model) else { continue }
                    out.append(found)
                default:
                    continue
                }
            default:
                continue
            }
        }
        return out
    }

    /// What a person's line draws as a bubble (nil = a marker, divider or
    /// command pill: not counted), `""` = a bubble with no searchable text.
    private static func bubbleText(_ text: String, model: AgentSessionModel) -> String? {
        if case .marker = TranscriptGrouping.feedInput(text) { return nil }
        if Chats.commandDivider(text) != nil { return nil }
        if SlashCommands.command(for: text, agent: model.catalogAgent) != nil { return nil }
        if let input = TranscriptGrouping.crossSessionInput(text) { return input.body }
        let parsed = SteerImageMessage.parse(text)
        guard !parsed.attachmentIds.isEmpty || !parsed.files.isEmpty else { return text }
        return parsed.markers.isEmpty ? parsed.text : ""
    }

    /// A plain tool line (`ToolRow`'s generic row): its title's matches and
    /// its output's stop; nil = a call drawn another way (one of our tools,
    /// a script card, an edit).
    private static func toolRow(_ item: AgentFeedItem, key: String, query: String, group: String?, model: AgentSessionModel) -> Row? {
        guard case let .tool(id, name, detail, _, callId, kind, settled, failed, diff, _, output, script, _) = item,
              diff?.isEmpty != false, script == nil, !ScriptCards.isScriptCall(item),
              failed || ExpToolDisplay.resolve(toolName: name) == nil else { return nil }
        let title = toolTitle(name: name, detail: detail, kind: kind, settled: settled, failed: failed, output: output, tests: callId.flatMap { model.testResults[$0] })
        let outputCount = output.map { $0.isEmpty ? 0 : FindMarks.count($0, query: query) } ?? 0
        return Row(
            row: FindInChat.Row(key: key, count: FindMarks.count(title, query: query), group: group, outputHit: outputCount > 0),
            itemId: id, outputCount: outputCount
        )
    }

    /// The title `ToolRow` paints: a settled test run's failed-tests line,
    /// else the call's own.
    static func toolTitle(name: String, detail: String?, kind: String?, settled: Bool, failed: Bool, output: String?, tests: TestResults?) -> String {
        if settled, kind == "execute", output?.isEmpty == false, let line = Trace.failedTests(tests) { return line.title }
        return Trace.tool(toolKind: kind ?? "other", name: name, detail: detail, settled: settled, failed: failed).title
    }
}

/// #EDE-130: wires the chat screen's find to the transcript: a new query
/// searches, a moved feed recounts, a step moves the current stop, a
/// landing reveals its row in the painted window, and the counter and
/// status line go back to the bar.
struct TranscriptFindDriver: ViewModifier {
    let find: ChatFind?
    let transcriptFind: TranscriptFind
    let model: AgentSessionModel
    let accountId: String
    let rootId: String
    let sessionId: String

    func body(content: Content) -> some View {
        let query = find?.query ?? ""
        content
            .onChange(of: query, initial: true) { _, next in
                transcriptFind.deviceName = model.hostDevice.displayLabel
                transcriptFind.search(next, model: model, accountId: accountId, rootId: rootId, sessionId: sessionId)
            }
            .onChange(of: query.isEmpty ? -1 : model.findRevision) { _, _ in
                transcriptFind.recount(model: model, sessionId: sessionId)
            }
            .onChange(of: find?.step) { _, step in
                transcriptFind.step(step)
            }
            .onChange(of: transcriptFind.report()) { _, report in
                find?.setReport(report)
            }
            .onChange(of: transcriptFind.focus) { _, focus in
                guard let focus, let id = transcriptFind.itemId(focus.id) else { return }
                model.revealFind(id)
            }
    }
}
