import ExpCore
import ExpUI
import SwiftUI
import UIKit

/// S-3 STORY VIEWER (stories.json, styleguide `story-viewer`): full screen
/// over everything on the phone. Top: THE StoryProgress over the ring's
/// stories, then the header (the ring's avatar, the story's session name,
/// its meta muted, the close ghost glyph; a tap on the avatar or name opens
/// that session). Body: the one-line title (none over plan text), then the pictures (fitted, a
/// pager when several, the caption under it) and/or the surface through THE
/// surface card renderer, else the plan's markdown. Footer per `footer`:
/// Approve · Change, the question's chips (+ the free text when custom), a
/// reply field, or nothing. NAVIGATION: the right third / a swipe left =
/// next (past the ring's last = the next ring, past the last ring = close),
/// the left third / a swipe right = previous, a swipe down = close, a held
/// press pauses; only a result advances on its own (`autoAdvanceMs`). The
/// order is the rings' as the viewer OPENED (marking a story seen reorders
/// the live fold); an answered story drops out and the viewer moves on.
struct StoryViewer: View {
    /// The live rings (the fold recomputes from sync while the viewer is up).
    let rings: [Stories.StoryRing]
    let startKey: String
    /// stories.json STORIES TAB: a tile opens the viewer at its story (and
    /// a thumbnail at its picture); nil = the ring's first story.
    var startStory: String?
    var startPicture = 0
    var card: (String) -> TreeMessageCard? = { _ in nil }
    var avatar: (Stories.StoryRing) -> StoryRingAvatar
    /// A specimen's frozen progress fill (stories.json `specimen.viewer`).
    var fraction: Double?
    /// A story was SHOWN (once per story while the viewer is up).
    var onSeen: (Stories.Story) -> Void = { _ in }
    /// The card answer (`sessions.answer`); true = sent.
    var onAnswer: (ThreadCardAnswer) async -> Bool = { _ in false }
    var onOpenSession: (Stories.Story) -> Void = { _ in }
    var onClose: () -> Void

    private struct Entry: Equatable {
        let ringKey: String
        let storyId: String
    }

    @State private var order: [Entry] = []
    @State private var position = 0
    @State private var elapsed: Double = 0
    @State private var paused = false
    @State private var shown: Set<String> = []
    @State private var holdTask: Task<Void, Never>?
    @State private var held = false
    @State private var pagerFrame: CGRect = .zero
    @State private var scrollAtTop = true

    private static let space = "story-viewer"
    private static let tick: Double = 50
    private static let hold: Duration = .milliseconds(250)
    private static let swipe: CGFloat = 60
    private static let avatarSize: CGFloat = 36

    // MARK: - Live lookup

    private var storiesById: [String: (ring: Stories.StoryRing, story: Stories.Story)] {
        var out: [String: (ring: Stories.StoryRing, story: Stories.Story)] = [:]
        for ring in rings {
            for story in ring.stories { out[story.id] = (ring, story) }
        }
        return out
    }

    private func present(_ index: Int, _ live: [String: (ring: Stories.StoryRing, story: Stories.Story)]) -> Bool {
        order.indices.contains(index) && live[order[index].storyId] != nil
    }

    private var current: (ring: Stories.StoryRing, story: Stories.Story)? {
        guard order.indices.contains(position) else { return nil }
        return storiesById[order[position].storyId]
    }

    // MARK: - Body

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            AppBackground()
            if let current {
                let live = storiesById
                let segments = order.indices.filter { order[$0].ringKey == current.ring.key && present($0, live) }
                VStack(alignment: .leading, spacing: 12) {
                    StoryProgress(
                        count: segments.count,
                        index: segments.firstIndex(of: position) ?? 0,
                        fraction: fraction ?? current.story.autoAdvanceMs.map { elapsed / $0 } ?? 1
                    )
                    header(current.ring, current.story)
                    navigable(current.story)
                    StoryFooter(story: current.story, card: card(current.story.id), onAnswer: onAnswer)
                        .id(current.story.id)
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 12)
                .task(id: current.story.id) { await run(current.story) }
            }
        }
        .preferredColorScheme(.dark)
        .coordinateSpace(name: Self.space)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Stories.TestIds.viewer)
        .accessibilityAction(named: Stories.Copy.next) { next() }
        .accessibilityAction(named: Stories.Copy.previous) { previous() }
        .onAppear(perform: open)
        .onChange(of: rings) { _, _ in settle() }
    }

    private func header(_ ring: Stories.StoryRing, _ story: Stories.Story) -> some View {
        HStack(spacing: 10) {
            Button { onOpenSession(story) } label: {
                HStack(spacing: 10) {
                    StoryRing(stories: ring.stories, size: Self.avatarSize) { StoryRingAvatarView(avatar: avatar(ring)) }
                    VStack(alignment: .leading, spacing: 1) {
                        Text(story.sessionName)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        Text(story.meta)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            Spacer(minLength: 0)
            GhostIconButton(AppIcons.uiClose, accessibilityLabel: Stories.Copy.close, action: onClose)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Stories.TestIds.header)
    }

    /// The title + body: the tap thirds, the swipes and the held press.
    private func navigable(_ story: Stories.Story) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            // stories.json: no title line over the plan text (its first line).
            if let title = story.title, story.text == nil {
                Text(title)
                    .font(.headline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
            }
            StoryBody(
                story: story, card: card(story.id), pagerFrame: $pagerFrame, scrollAtTop: $scrollAtTop,
                space: Self.space, initialPage: story.id == startStory ? startPicture : 0
            )
            .id(story.id)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .contentShape(Rectangle())
        .simultaneousGesture(
            DragGesture(minimumDistance: 0, coordinateSpace: .named(Self.space))
                .onChanged { _ in
                    guard holdTask == nil else { return }
                    holdTask = Task {
                        try? await Task.sleep(for: Self.hold)
                        guard !Task.isCancelled else { return }
                        held = true
                        paused = true
                    }
                }
                .onEnded { value in end(value, story: story) }
        )
    }

    // MARK: - Navigation

    private func open() {
        guard order.isEmpty else { return }
        order = rings.flatMap { ring in ring.stories.map { Entry(ringKey: ring.key, storyId: $0.id) } }
        position = startStory.flatMap { id in order.firstIndex { $0.storyId == id } }
            ?? order.firstIndex { $0.ringKey == startKey } ?? 0
        if order.isEmpty { onClose() }
    }

    /// The live rings moved: the current story gone (answered) = the next
    /// one still there, none = close.
    private func settle() {
        let live = storiesById
        guard !present(position, live) else { return }
        if let next = order.indices.first(where: { $0 > position && present($0, live) }) {
            go(next)
        } else {
            onClose()
        }
    }

    private func next() {
        let live = storiesById
        if let next = order.indices.first(where: { $0 > position && present($0, live) }) {
            go(next)
        } else {
            onClose()
        }
    }

    private func previous() {
        let live = storiesById
        if let previous = order.indices.last(where: { $0 < position && present($0, live) }) {
            go(previous)
        } else {
            elapsed = 0
        }
    }

    private func go(_ index: Int) {
        position = index
        elapsed = 0
    }

    private func end(_ value: DragGesture.Value, story: Stories.Story) {
        holdTask?.cancel()
        holdTask = nil
        let wasHeld = held
        held = false
        paused = false
        let dx = value.translation.width
        let dy = value.translation.height
        if abs(dx) > Self.swipe, abs(dx) > 2 * abs(dy) {
            // A pager of pictures takes its own horizontal swipes.
            if story.pictures.count > 1, pagerFrame.contains(value.startLocation) { return }
            if dx < 0 { next() } else { previous() }
        } else if dy > Self.swipe, dy > 2 * abs(dx), scrollAtTop {
            onClose()
        } else if !wasHeld, abs(dx) < 10, abs(dy) < 10 {
            let width = UIScreen.main.bounds.width
            if value.startLocation.x > width * 2 / 3 {
                next()
            } else if value.startLocation.x < width / 3 {
                previous()
            }
        }
    }

    /// Marks the story seen once, then (a result) runs its timer.
    private func run(_ story: Stories.Story) async {
        if !shown.contains(story.id) {
            shown.insert(story.id)
            onSeen(story)
        }
        guard let duration = story.autoAdvanceMs else { return }
        while !Task.isCancelled {
            try? await Task.sleep(for: .milliseconds(Int(Self.tick)))
            guard !Task.isCancelled else { return }
            if paused { continue }
            elapsed += Self.tick
            if elapsed >= duration {
                next()
                return
            }
        }
    }
}

// MARK: - Body

/// A story's body: the pictures and/or the surface, else the plan text.
private struct StoryBody: View {
    let story: Stories.Story
    let card: TreeMessageCard?
    @Binding var pagerFrame: CGRect
    @Binding var scrollAtTop: Bool
    let space: String
    var initialPage = 0

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var page = 0
    @State private var placed = false
    @State private var handle = PlanCardSurfaceHandle()

    var body: some View {
        let surface = story.surface ? card?.surface : nil
        VStack(alignment: .leading, spacing: 12) {
            if !story.pictures.isEmpty {
                pictures
                    .frame(maxHeight: .infinity)
                if let surface {
                    surfaceCard(surface)
                        .fixedSize(horizontal: false, vertical: true)
                }
            } else if let surface {
                scroller { surfaceCard(surface) }
            } else if let text = story.text {
                scroller {
                    AgentMarkdownText(
                        text: PlanCard.renderableMarkdown(text),
                        context: AgentMarkdownContext(
                            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                            accountId: accountId,
                            httpClient: deps.httpClient
                        ),
                        options: [.hardLineBreaks]
                    )
                }
            } else {
                Spacer(minLength: 0)
            }
        }
        .onAppear {
            scrollAtTop = true
            guard !placed else { return }
            placed = true
            page = min(max(0, initialPage), max(0, story.pictures.count - 1))
        }
    }

    private func surfaceCard(_ surface: CardSurface) -> some View {
        SurfaceCardFrame(title: nil, animate: false) {
            PlanCardSurfaceView(surface: surface, handle: handle, editable: false, bare: true)
        }
    }

    private func scroller<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        ScrollView {
            content()
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(GeometryReader { proxy in
                    Color.clear.preference(key: StoryScrollTop.self, value: proxy.frame(in: .named("story-body")).minY)
                })
        }
        .coordinateSpace(name: "story-body")
        .onPreferenceChange(StoryScrollTop.self) { scrollAtTop = $0 >= -1 }
    }

    @ViewBuilder
    private var pictures: some View {
        let pictures = story.pictures
        VStack(spacing: 8) {
            Group {
                if pictures.count == 1 {
                    StoryPicture(picture: pictures[0])
                } else {
                    TabView(selection: $page) {
                        ForEach(Array(pictures.enumerated()), id: \.offset) { index, picture in
                            StoryPicture(picture: picture).tag(index)
                        }
                    }
                    .tabViewStyle(.page(indexDisplayMode: .always))
                }
            }
            .background(GeometryReader { proxy in
                Color.clear.onAppear { pagerFrame = proxy.frame(in: .named(space)) }
                    .onChange(of: proxy.frame(in: .named(space))) { _, frame in pagerFrame = frame }
            })
            if let caption = pictures[min(page, pictures.count - 1)].caption, !caption.isEmpty {
                Text(caption)
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(maxWidth: .infinity)
                    .multilineTextAlignment(.center)
            }
        }
    }
}

private struct StoryScrollTop: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}

/// One picture through the instance's attachment loader (the plan card's
/// picture tiles' loader), FITTED; the muted placeholder while it loads.
struct StoryPicture: View {
    let picture: Stories.Picture

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var image: UIImage?

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: GlassTokens.groupRadius)
        ZStack {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFit()
                    .clipShape(shape)
            } else {
                shape
                    .fill(GlassTokens.fillCard)
                    .overlay(shape.stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
                    .overlay(
                        AppIcon(AppIcons.editorImage, size: 28)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    )
                    .aspectRatio(aspect, contentMode: .fit)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityLabel(picture.caption ?? picture.label)
        .task(id: picture.attachmentId) {
            let loader = AttachmentImageLoader(
                baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                accountId: accountId,
                httpClient: deps.httpClient,
                pendingImages: [:]
            )
            image = try? await loader.load("/api/attachments/\(picture.attachmentId)")
        }
    }

    private var aspect: CGFloat {
        guard let width = picture.width, let height = picture.height, width > 0, height > 0 else { return 4 / 3 }
        return width / height
    }
}

// MARK: - Footer

/// The story's footer: THE plan options (Approve = approve, Change = reject
/// + its feedback field), the question's option chips (+ the free text when
/// custom), a reply field, or nothing. Answers go through `sessions.answer`.
private struct StoryFooter: View {
    let story: Stories.Story
    let card: TreeMessageCard?
    let onAnswer: (ThreadCardAnswer) async -> Bool

    @State private var draft = PlanCardDraft()
    @State private var rejecting = false
    @State private var reply = ""
    @State private var busy = false

    var body: some View {
        switch story.footer {
        case .plan: plan
        case .choices: choices
        case .reply: replyField
        case .none: EmptyView()
        }
    }

    private var plan: some View {
        VStack(alignment: .trailing, spacing: PlanCard.Stepper.buttonGap) {
            if rejecting {
                PlanCardField(
                    placeholder: AskDock.Copy.feedbackPlaceholder,
                    text: $draft.feedback,
                    multiline: true,
                    identifier: PlanCard.TestIds.feedback
                )
            }
            HStack(spacing: PlanCard.Stepper.buttonGap) {
                Spacer(minLength: 0)
                if rejecting {
                    GlassPill(PlanCard.Copy.cancel, size: .md, mode: .action { rejecting = false })
                    GlassPill(
                        Stories.Copy.change, size: .md, mode: .action { decide(AskDock.planRejectKey) },
                        primary: true, enabled: !busy
                    )
                    .accessibilityIdentifier("story-change")
                } else {
                    GlassPill(Stories.Copy.change, size: .md, mode: .action { rejecting = true }, enabled: !busy)
                        .accessibilityIdentifier("story-change")
                    GlassPill(
                        Stories.Copy.approve, size: .md, mode: .action { decide("approve") },
                        primary: true, enabled: !busy
                    )
                    .accessibilityIdentifier("story-approve")
                }
            }
        }
    }

    @ViewBuilder
    private var choices: some View {
        if let card {
            let options = card.options
            let issue = PlanCard.answerIssue(
                options: options.map(\.label), multi: card.multi, custom: card.custom,
                input: PlanCard.AnswerInput(labels: draft.answer(""))
            )
            VStack(alignment: .trailing, spacing: 8) {
                PlanCardQuestionBlock(
                    questionId: "",
                    prompt: nil,
                    caption: card.multi ? PlanCard.Copy.chooseAny : nil,
                    multi: card.multi,
                    custom: card.custom,
                    options: options,
                    picked: draft.answer(""),
                    editable: !busy,
                    customText: Binding(get: { draft.custom[""] ?? "" }, set: { draft.custom[""] = $0 }),
                    onToggle: { draft.toggle("", label: $0, multi: card.multi, order: options.map(\.label)) },
                    onType: { draft.type("", text: $0, multi: card.multi) }
                )
                GlassPill(
                    PlanCard.Copy.answer, size: .md, mode: .action { submit(draft.answer("")) },
                    primary: true, enabled: issue == nil && !busy
                )
                .accessibilityIdentifier("story-answer")
            }
        }
    }

    private var replyField: some View {
        GlassTextField(
            Stories.Copy.reply,
            text: $reply,
            lines: 1...4,
            accessibilityIdentifier: "story-reply"
        ) {
            EmptyView()
        } trailing: {
            GhostIconButton(
                AppIcons.uiSend, accessibilityLabel: PlanCard.Copy.answer,
                enabled: !busy && !reply.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            ) { submit([reply]) }
        }
    }

    private func decide(_ key: String) {
        guard let option = AskDock.planOption(key: key) else { return }
        let approved = option.decision == PlanDecision.approved.rawValue
        let typed = approved ? "" : draft.feedback.trimmingCharacters(in: .whitespacesAndNewlines)
        send(ThreadCardAnswer(
            messageId: story.id,
            decision: approved ? .approved : .rejected,
            fresh: approved && option.fresh,
            feedback: typed.isEmpty ? nil : typed
        ))
    }

    private func submit(_ labels: [String]) {
        let picked = labels.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
        guard !picked.isEmpty else { return }
        send(ThreadCardAnswer(messageId: story.id, answer: PlanCard.joinAnswers(picked), answers: .question(picked)))
    }

    private func send(_ answer: ThreadCardAnswer) {
        guard !busy else { return }
        busy = true
        Task {
            // Sent = the story drops out once the answer syncs back.
            _ = await onAnswer(answer)
            busy = false
        }
    }
}
