import ExpCore
import ExpUI
import SwiftUI
#if canImport(ExponentialUI)
import ExponentialUI
import ExponentialUICore
#endif

// #EXP-2067 (chat-express.json, ×4): an agent's text with Express blocks
// draws as its segments in order: each prose run THE message bubble, each
// block ONE item in the agent's column, one surface card frame per surface
// of the block drawn by THE plan card painter (`PlanCardSurfaceHost`), the
// inline marker under a failed one, the surface card skeleton while an open
// block has no surface yet. The split and the compile are the eui SDK's
// (`ChatExpress.Row` over the FFI), one stream per row key across renders.

/// The SDK over the FFI; nil = the painter is not linked (the rows then
/// read their pure prose).
enum ExpressSdks {
    static let ffi: ExpressJSONSdk? = {
        #if canImport(ExponentialUI)
        ExpressJSONSdk(
            unwrapJson: { expressUnwrapJson(text: $0) },
            stream: { surfaceId in
                let decoder = ExpressDecoder(wrapped: false, surfaceId: surfaceId)
                return (push: { decoder.push(chunk: $0) }, end: { decoder.end() })
            }
        )
        #else
        nil
        #endif
    }()
}

/// Every press on a genUI surface in a transcript: THE action card posted as
/// the viewer through the composer's send path (`sessions.send`, sealed in a
/// keyed tree), or THE failed-action toast.
enum ExpressActions {
    #if canImport(ExponentialUI)
    /// The renderer's event as the reference reads it: a context object (an
    /// empty one = none), the resolved `userMessage`.
    static func event(_ event: SurfaceActionEvent) -> ChatExpress.ActionEvent {
        var context: CardJSON?
        if case let .object(fields) = event.context, !fields.isEmpty,
           let object = try? JSONSerialization.jsonObject(with: Data(event.context.json.utf8)) {
            context = CardJSON(any: object)
        }
        return ChatExpress.ActionEvent(name: event.name, context: context, userMessage: event.userMessage)
    }
    #endif

    @MainActor
    static func post(
        _ event: ChatExpress.ActionEvent,
        origin: ChatExpress.ActionOrigin,
        sessionId: String,
        accountId: String,
        api: SessionsApi,
        toaster: Toaster
    ) {
        switch ChatExpress.surfaceAction(event, origin: origin) {
        case nil:
            return
        case .tooLarge:
            toaster.error(ChatExpress.Copy.actionTooLarge)
        case let .post(body, card):
            Task {
                do {
                    _ = try await api.send(
                        accountId: accountId, .message(sessionId: sessionId, text: body.isEmpty ? nil : body),
                        actionCard: card
                    )
                } catch {
                    toaster.error(
                        FeedbackStates.Errors.title(verb: "send", thing: "the message"),
                        description: error.userFacingMessage
                    )
                }
            }
        }
    }
}

/// The rows' Express state across renders: one `ChatExpress.Row` per row
/// key (a stream pushes only a block's new suffix), each block's renderer
/// host, and which surfaces entered live.
@MainActor
final class ExpressRowStore {
    static let shared = ExpressRowStore()

    private struct Entry {
        let row: ChatExpress.Row
        var text: String
        var final: Bool
        var segments: [ChatExpress.RowSegment]?
        /// First seen while streaming: its surfaces enter.
        let live: Bool
    }

    private var entries: [String: Entry] = [:]
    private var order: [String] = []
    private static let cap = 400

    /// A row's segments (nil = no block: today's bubble). A streaming row's
    /// held tail (`<a2` arriving) draws the prose before it.
    func segments(key: String, text: String, final: Bool) -> [ChatExpress.RowSegment]? {
        guard ChatExpress.hasExpressBlock(text) else {
            entries[key] = nil
            let shown = ChatExpress.withoutHeldTail(text, final: final)
            guard shown.utf16.count != text.utf16.count else { return nil }
            let prose = shown.trimmingCharacters(in: .whitespacesAndNewlines)
            return prose.isEmpty ? [] : [.prose(prose)]
        }
        guard let sdk = ExpressSdks.ffi else { return [.prose(ChatExpress.plainProse(text))] }
        if let entry = entries[key], entry.final == final, entry.text.utf16.elementsEqual(text.utf16) {
            return entry.segments
        }
        let held = entries[key]
        let row = held?.row ?? ChatExpress.Row(rowKey: key, sdk: sdk)
        let segments = row.update(text, final: final)
        entries[key] = Entry(row: row, text: text, final: final, segments: segments, live: held?.live ?? !final)
        if held == nil {
            order.append(key)
            if order.count > Self.cap {
                let drop = order.removeFirst()
                entries[drop] = nil
                hosts = hosts.filter { !$0.key.hasPrefix("\(drop)#") }
            }
        }
        return segments
    }

    /// The row streamed in while its chat was open.
    func live(_ rowKey: String) -> Bool { entries[rowKey]?.live ?? false }

    private var entering: Set<String> = []
    private var settled: Set<String> = []

    /// A surface enters ONCE, when its row arrived live; history, a reopen, a
    /// restart or a re-split never re-enters.
    func enters(rowKey: String, block: String, surfaceId: String) -> Bool {
        let id = "\(block)|\(surfaceId)"
        if entering.contains(id) { return true }
        guard !settled.contains(id) else { return false }
        guard live(rowKey) else {
            settled.insert(id)
            return false
        }
        entering.insert(id)
        let enter = Chats.SurfaceEnter.Frame.duration
        let seconds = enter == "slow" ? DesignTokens.Motion.Duration.slow : DesignTokens.Motion.Duration.standard
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(seconds + 0.2))
            self.entering.remove(id)
            self.settled.insert(id)
        }
        return true
    }

    #if canImport(ExponentialUI)
    /// One block's renderer: its own host (theme exponential, dark) fed the
    /// block's messages in order; a restarted stream (`generation`) starts
    /// a fresh one.
    @MainActor
    final class BlockHost {
        let host: ExponentialHost
        let plugin: PlanCardSurfaceHost
        let generation: Int
        var applied = 0

        init(plugin: PlanCardSurfaceHost, generation: Int) {
            self.plugin = plugin
            self.generation = generation
            host = ExponentialHost(HostOptions(theme: ThemeHandle.builtin("exponential"), mode: .dark, plugin: plugin))
        }
    }

    private var hosts: [String: BlockHost] = [:]

    func host(_ block: ChatExpress.BlockState) -> BlockHost? {
        hosts[block.key].flatMap { $0.generation == block.generation ? $0 : nil }
    }

    /// The block's host, created on first use, caught up with its messages.
    @discardableResult
    func apply(_ block: ChatExpress.BlockState, plugin: () -> PlanCardSurfaceHost) -> BlockHost {
        var current = host(block)
        if current == nil || current!.applied > block.messages.count {
            current = BlockHost(plugin: plugin(), generation: block.generation)
            hosts[block.key] = current
        }
        let held = current!
        while held.applied < block.messages.count {
            held.host.receive(block.messages[held.applied].compactString)
            held.applied += 1
        }
        return held
    }
    #else
    private var hosts: [String: Never] = [:]
    #endif
}

/// One block where it stands: a surface card frame per surface (creation
/// order, `layout.blockGap` apart), THE inline marker under a failed /
/// tooMany / tooLarge one, THE surface card skeleton while an open block
/// has no surface yet. `onAction` = a press (nil = the viewer may not post).
struct ExpressBlockView: View {
    let rowKey: String
    let block: ChatExpress.BlockState
    /// The synced row's `messages.id` (nil = a live feed row).
    let messageId: String?
    var onAction: ((ChatExpress.ActionEvent, ChatExpress.ActionOrigin) -> Void)?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.cardTeamId) private var cardTeamId
    @Environment(\.cardTeamIds) private var cardTeamIds
    /// Bumped once the block's host has caught up (it paints a frame later).
    @State private var caughtUp = 0

    var body: some View {
        VStack(alignment: .leading, spacing: CGFloat(ChatExpress.Layout.blockGap)) {
            surfaces
            if block.status == .open, block.surfaces.isEmpty {
                SurfaceCardFrame(title: nil, animate: false) {
                    SkeletonRows(rows: 2, rowHeight: 24, dot: false, dividers: false)
                }
            }
            if block.marked {
                TranscriptMarkerRow(label: ChatExpress.Copy.blockFailed)
                    .accessibilityIdentifier("express-block-failed")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        #if canImport(ExponentialUI)
        .task(id: "\(block.key)#\(block.generation)#\(block.messages.count)") {
            let applied = ExpressRowStore.shared.apply(block) { makePlugin() }
            applied.plugin.onEvent = actionHandler
            caughtUp &+= 1
        }
        .onChange(of: onAction == nil) { _, _ in ExpressRowStore.shared.host(block)?.plugin.onEvent = actionHandler }
        #endif
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("express-block")
        .accessibilityValue(block.status.rawValue)
    }

    @ViewBuilder
    private var surfaces: some View {
        #if canImport(ExponentialUI)
        let store = ExpressRowStore.shared
        let held = caughtUp >= 0 ? store.host(block) : nil
        ForEach(block.surfaces, id: \.self) { surfaceId in
            SurfaceCardFrame(title: nil, animate: store.enters(rowKey: rowKey, block: block.key, surfaceId: surfaceId)) {
                if let held {
                    HostSurface(host: held.host, surfaceId: surfaceId)
                        .allowsHitTesting(onAction != nil)
                } else {
                    Color.clear.frame(height: 1)
                }
            }
        }
        #endif
    }

    private var actionHandler: ((ChatExpress.ActionEvent, String) -> Void)? {
        guard let onAction else { return nil }
        let origin = (block: block.key, messageId: messageId)
        return { event, surfaceId in
            onAction(event, ChatExpress.ActionOrigin(surfaceId: surfaceId, block: origin.block, messageId: origin.messageId))
        }
    }

    #if canImport(ExponentialUI)
    private func makePlugin() -> PlanCardSurfaceHost {
        PlanCardSurfaceHost(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            teamId: cardTeamId,
            teamIds: cardTeamIds,
            deps: deps,
            httpClient: deps.httpClient
        )
    }
    #endif
}

/// #EXP-2074 (chat-express.json WHICH ROWS: "a feed `narration` row (main
/// lane or a subagent's)"): how a subagent lane's narration rows read their
/// blocks, the main lane's path: its row key (`expressRowKey`), FINAL by
/// the lane's own rule (a row of the lane follows, or the turn ended / the
/// subagent is done), its presses through the main lane's handler.
struct ExpressLane {
    /// (the narration's row id, its messageId) → the row key + the synced
    /// row's `messages.id` (nil = a live feed row).
    let key: @MainActor (Int, String?) -> (key: String, messageId: String?)
    let final: Bool
    var onAction: ((ChatExpress.ActionEvent, ChatExpress.ActionOrigin) -> Void)?

    /// The row's segments (nil = no block: today's bubble).
    @MainActor
    func split(id: Int, messageId: String?, text: String) -> (key: String, messageId: String?, segments: [ChatExpress.RowSegment])? {
        let row = key(id, messageId)
        guard let segments = ExpressRowStore.shared.segments(key: row.key, text: text, final: final) else { return nil }
        return (row.key, row.messageId, segments)
    }
}

/// An agent text's segments in its sender group: the prose runs THE message
/// bubble (`bubble(text, first, last)`: the name and tail on the row's first
/// segment only, the `HH:mm` on its LAST prose bubble), each block where it
/// stands.
struct ExpressSegmentsView<Bubble: View>: View {
    let rowKey: String
    let segments: [ChatExpress.RowSegment]
    let messageId: String?
    var onAction: ((ChatExpress.ActionEvent, ChatExpress.ActionOrigin) -> Void)?
    /// (text, the row's first segment, the row's last prose run)
    @ViewBuilder let bubble: (String, Bool, Bool) -> Bubble

    var body: some View {
        let lastProse = segments.lastIndex { if case .prose = $0 { true } else { false } }
        VStack(alignment: .leading, spacing: TranscriptGrouping.Layout.gapInGroup) {
            ForEach(Array(segments.enumerated()), id: \.offset) { index, segment in
                switch segment {
                case let .prose(text):
                    bubble(text, index == 0, index == lastProse)
                case let .block(block):
                    ExpressBlockView(rowKey: rowKey, block: block, messageId: messageId, onAction: onAction)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
