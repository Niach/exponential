import ExpCore
import ExpUI
import SwiftUI
import UIKit

/// S-3 STORY VIEWER (stories.json, styleguide `story-viewer`; #EXP-1493 ONE
/// FRAME PER STORY): full screen over everything on the phone, Instagram-like.
/// The viewer steps through the ring's stories (`Stories.frames`): a PLAN =
/// its cover (the first picture fitted on black, the counts line, up to 4
/// thumbnails + `+N`, ONE full-width `Open plan` opening THE plan page); a
/// GALLERY = the story's pictures, one at a time (a swipe inside steps the
/// picture first, the "i / n" counter top-right, the caption muted under it,
/// the filmstrip over the bottom scrim when n > 1); a SURFACE through THE
/// surface renderer with no card chrome; a TITLE = nothing. THE StoryProgress
/// (one segment per story), the header (the ring's avatar, the story's
/// session name, its meta muted, the close ghost glyph; a tap on the avatar
/// or name opens that session), the KIND BADGE leading the title, and the
/// description FLOAT over the top on a scrim; the footer floats over the
/// bottom on the mirrored scrim. NAVIGATION: the right third / a swipe left
/// = next (a gallery's next picture first; past the ring's last = the next
/// ring, past the last ring = close), the left third / a swipe right =
/// previous, a swipe down = close, a held press pauses; a gallery's timer
/// steps its pictures every `autoAdvanceMs` then the story (a decision's
/// never). The order is the rings' as the viewer OPENED; an answered story
/// drops out and the viewer moves on.
struct StoryViewer: View {
    /// The live rings (the fold recomputes from sync while the viewer is up).
    let rings: [Stories.StoryRing]
    let startKey: String
    /// stories.json STORIES TAB: a tile opens the viewer at its story's
    /// frame, a thumbnail at that picture of its gallery; nil = the ring's first.
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
    /// #EXP-1493: a plan frame's `Open plan` (the host closes the viewer and
    /// pushes THE plan page).
    var onOpenPlan: (Stories.Story) -> Void = { _ in }
    var onClose: () -> Void

    private struct Entry: Equatable {
        let ringKey: String
        let storyId: String
    }

    private typealias Live = [String: (ring: Stories.StoryRing, story: Stories.Story)]

    @State private var order: [Entry] = []
    @State private var position = 0
    /// A gallery's current picture.
    @State private var picture = 0
    @State private var elapsed: Double = 0
    @State private var paused = false
    @State private var shown: Set<String> = []
    @State private var holdTask: Task<Void, Never>?
    @State private var held = false
    @State private var scrollAtTop = true
    @State private var topHeight: CGFloat = 0
    @State private var footerHeight: CGFloat = 0

    private static let space = "story-viewer"
    private static let tick: Double = 50
    private static let hold: Duration = .milliseconds(250)
    private static let swipe: CGFloat = 60
    private static let avatarSize: CGFloat = 36

    // MARK: - Live lookup

    private var storiesById: Live {
        var out: Live = [:]
        for ring in rings {
            for story in ring.stories { out[story.id] = (ring, story) }
        }
        return out
    }

    private func present(_ index: Int, _ live: Live) -> Bool {
        order.indices.contains(index) && live[order[index].storyId] != nil
    }

    private var current: (ring: Stories.StoryRing, story: Stories.Story)? {
        guard order.indices.contains(position) else { return nil }
        return storiesById[order[position].storyId]
    }

    /// The current ring's stories in the opened order (the live ones).
    private func ringStories(_ ringKey: String, _ live: Live) -> [Stories.Story] {
        order.filter { $0.ringKey == ringKey }.compactMap { live[$0.storyId]?.story }
    }

    // MARK: - Body

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            if let current {
                let live = storiesById
                let frames = Stories.frames(ringStories(current.ring.key, live))
                let at = max(0, Stories.frameIndex(frames, storyId: current.story.id))
                if frames.indices.contains(at) {
                    let shownFrame = frames[at]
                    navigable(current.story, shownFrame)
                        .task(id: "\(current.story.id):\(picture)") { await run(current.story, shownFrame) }
                    top(current.ring, current.story, shownFrame, frames: frames, at: at)
                    footer(current.story, shownFrame)
                }
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

    /// The current segment's fill: a gallery = (picture + its fraction) / n.
    private func progress(_ shownFrame: Stories.Frame) -> Double {
        if let fraction { return fraction }
        let own = shownFrame.autoAdvanceMs.map { elapsed / $0 } ?? 1
        guard shownFrame.kind == .gallery, shownFrame.pictures > 0 else { return own }
        return (Double(picture) + min(own, 1)) / Double(shownFrame.pictures)
    }

    /// The floating top band: progress, header, the badge + title and the
    /// description, on the scrim.
    private func top(
        _ ring: Stories.StoryRing, _ story: Stories.Story, _ shownFrame: Stories.Frame, frames: [Stories.Frame],
        at: Int
    ) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            StoryProgress(count: frames.count, index: at, fraction: progress(shownFrame))
                .allowsHitTesting(false)
            header(ring, story)
            VStack(alignment: .leading, spacing: 6) {
                StoryKindBadge(kind: story.kind)
                if let title = shownFrame.title {
                    Text(title)
                        .font(.headline)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .accessibilityIdentifier("story-title")
                }
                if let description = shownFrame.description {
                    Text(description)
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(3)
                        .accessibilityIdentifier("story-description")
                }
            }
            .padding(.horizontal, 2)
            .allowsHitTesting(false)
        }
        .padding(.horizontal, 12)
        .padding(.top, 8)
        .padding(.bottom, 12)
        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { topHeight = $0 }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(alignment: .top) { StoryScrim(edge: .top).allowsHitTesting(false) }
        .frame(maxHeight: .infinity, alignment: .top)
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

    /// The footer floating over the bottom on the mirrored scrim: a plan's
    /// cover footer, a gallery's filmstrip, the story's answer footer.
    @ViewBuilder
    private func footer(_ story: Stories.Story, _ shownFrame: Stories.Frame) -> some View {
        let filmstrip = shownFrame.kind == .gallery && story.pictures.count > 1
        if shownFrame.kind == .plan || filmstrip || shownFrame.footer == .choices || shownFrame.footer == .reply {
            VStack(alignment: .leading, spacing: 12) {
                if shownFrame.kind == .plan {
                    planFooter(story)
                } else {
                    if filmstrip {
                        StoryThumbnailStrip(
                            count: story.pictures.count, current: picture, height: 52,
                            onPick: { index in
                                picture = index
                                elapsed = 0
                            }
                        ) { index in
                            StoryPicture(picture: story.pictures[index], fill: true, cover: true)
                        }
                        .frame(maxWidth: .infinity)
                    }
                    if shownFrame.footer != .none {
                        StoryFooter(story: story, footer: shownFrame.footer, card: card(story.id), onAnswer: onAnswer)
                            .id(story.id)
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 12)
            .padding(.bottom, 12)
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { footerHeight = $0 }
            .frame(maxWidth: .infinity)
            .background(alignment: .bottom) { StoryScrim(edge: .bottom).allowsHitTesting(false) }
            .frame(maxHeight: .infinity, alignment: .bottom)
        }
    }

    /// A plan cover's foot: the counts line, up to 4 thumbnails + `+N`, ONE
    /// full-width primary `Open plan`.
    private func planFooter(_ story: Stories.Story) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            if let counts = story.counts, !PlanPage.countsLine(counts).isEmpty {
                Text(PlanPage.countsLine(counts))
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .accessibilityIdentifier("story-plan-counts")
            }
            if !story.pictures.isEmpty {
                StoryThumbnailStrip(
                    count: story.pictures.count, current: 0, height: 40, width: 56, limit: 4,
                    onPick: { _ in onOpenPlan(story) }
                ) { index in
                    StoryPicture(picture: story.pictures[index], fill: true, cover: true)
                }
            }
            GlassSubmitButton(PlanPage.Copy.openPlan) { onOpenPlan(story) }
                .accessibilityIdentifier(PlanPage.TestIds.openPlan)
        }
    }

    /// The frame's body under the bands: the tap thirds, the swipes and the
    /// held press.
    private func navigable(_ story: Stories.Story, _ shownFrame: Stories.Frame) -> some View {
        StoryFrameBody(
            story: story, frame: shownFrame, picture: picture, card: card(story.id), top: topHeight,
            bottom: footerHeight, scrollAtTop: $scrollAtTop
        )
        .id("\(story.id):\(picture)")
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
                .onEnded { end($0, openPlan: shownFrame.kind == .plan ? { onOpenPlan(story) } : nil) }
        )
    }

    // MARK: - Navigation

    /// The steps inside a story: a gallery's pictures, else one.
    private func pictureCount(_ story: Stories.Story) -> Int {
        story.kind != .plan ? max(1, story.pictures.count) : 1
    }

    private func open() {
        guard order.isEmpty else { return }
        order = rings.flatMap { ring in ring.stories.map { Entry(ringKey: ring.key, storyId: $0.id) } }
        position = startStory.flatMap { id in order.firstIndex { $0.storyId == id } }
            ?? order.firstIndex { $0.ringKey == startKey } ?? 0
        // A thumbnail opens its story's frame with the gallery at that picture.
        if let startStory, order.indices.contains(position), order[position].storyId == startStory,
           let story = storiesById[startStory]?.story {
            picture = min(max(0, startPicture), pictureCount(story) - 1)
        }
        if order.isEmpty { onClose() }
    }

    /// The live rings moved: the current story gone (answered) = the next
    /// one still there, none = close.
    private func settle() {
        let live = storiesById
        guard !present(position, live) else { return }
        if let next = order.indices.first(where: { $0 > position && present($0, live) }) {
            go(next, picture: 0)
        } else {
            onClose()
        }
    }

    private func next() {
        let live = storiesById
        if let story = current?.story, picture + 1 < pictureCount(story) {
            picture += 1
            elapsed = 0
        } else if let next = order.indices.first(where: { $0 > position && present($0, live) }) {
            go(next, picture: 0)
        } else {
            onClose()
        }
    }

    private func previous() {
        let live = storiesById
        if picture > 0 {
            picture -= 1
            elapsed = 0
        } else if let previous = order.indices.last(where: { $0 < position && present($0, live) }) {
            go(previous, picture: 0)
        } else {
            elapsed = 0
        }
    }

    private func go(_ index: Int, picture: Int) {
        position = index
        self.picture = picture
        elapsed = 0
        scrollAtTop = true
    }

    /// `openPlan` = a plan frame: a tap ANYWHERE on its cover opens the plan
    /// (stories.json: the cover is one target, never the tap thirds).
    private func end(_ value: DragGesture.Value, openPlan: (() -> Void)? = nil) {
        holdTask?.cancel()
        holdTask = nil
        let wasHeld = held
        held = false
        paused = false
        let dx = value.translation.width
        let dy = value.translation.height
        if abs(dx) > Self.swipe, abs(dx) > 2 * abs(dy) {
            if dx < 0 { next() } else { previous() }
        } else if dy > Self.swipe, dy > 2 * abs(dx), scrollAtTop {
            onClose()
        } else if !wasHeld, abs(dx) < 10, abs(dy) < 10, let openPlan {
            openPlan()
        } else if !wasHeld, abs(dx) < 10, abs(dy) < 10 {
            let width = UIScreen.main.bounds.width
            if value.startLocation.x > width * 2 / 3 {
                next()
            } else if value.startLocation.x < width / 3 {
                previous()
            }
        }
    }

    /// Marks the frame's story seen once, then runs the timer (a gallery's
    /// per picture).
    private func run(_ story: Stories.Story, _ shownFrame: Stories.Frame) async {
        if !shown.contains(story.id) {
            shown.insert(story.id)
            onSeen(story)
        }
        guard fraction == nil, let duration = shownFrame.autoAdvanceMs else { return }
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

/// A floating band's scrim (`viewer.scrimHeight`): black 0.88 at the screen
/// edge, 0.8 behind the band's text, fading clear over the last
/// `viewer.scrimFade` past it, so a picture's own text never collides with
/// the title. It reaches behind the status bar / home indicator.
private struct StoryScrim: View {
    let edge: VerticalEdge

    var body: some View {
        let fade = CGFloat(Stories.Viewer.scrimFade)
        GeometryReader { proxy in
            let total = max(proxy.size.height + fade, CGFloat(Stories.Viewer.scrimHeight))
            let solid = max(0, (total - fade) / total)
            LinearGradient(
                stops: [
                    .init(color: .black.opacity(0.88), location: 0),
                    .init(color: .black.opacity(0.8), location: solid),
                    .init(color: .black.opacity(0), location: 1),
                ],
                startPoint: edge == .top ? .top : .bottom,
                endPoint: edge == .top ? .bottom : .top
            )
            .frame(height: total)
            .frame(maxHeight: .infinity, alignment: edge == .top ? .top : .bottom)
        }
        .ignoresSafeArea(edges: edge == .top ? .top : .bottom)
    }
}

// MARK: - Body

/// One frame's body: a plan's cover (its first picture fitted on black; a
/// tap anywhere on it = Open plan, `end`), a gallery's current picture fitted on black edge to
/// edge (the "i / n" counter, its caption under it), THE surface with no
/// card chrome, or nothing (a `title` frame).
private struct StoryFrameBody: View {
    let story: Stories.Story
    let frame: Stories.Frame
    let picture: Int
    let card: TreeMessageCard?
    /// The floating bands' heights (a scrolling body starts under the top
    /// band and ends above the footer).
    let top: CGFloat
    let bottom: CGFloat
    @Binding var scrollAtTop: Bool

    @State private var handle = PlanCardSurfaceHandle()

    var body: some View {
        switch frame.kind {
        case .plan:
            if let first = story.pictures.first {
                StoryPicture(picture: first, fill: true)
                    .ignoresSafeArea()
            } else {
                Color.clear
            }
        case .gallery:
            if story.pictures.indices.contains(picture) {
                let shown = story.pictures[picture]
                ZStack {
                    StoryPicture(picture: shown, fill: true)
                        .ignoresSafeArea()
                    VStack(spacing: 0) {
                        Spacer(minLength: top)
                        HStack {
                            Spacer()
                            if story.pictures.count > 1 {
                                Text("\(picture + 1) / \(story.pictures.count)")
                                    .font(.caption2.weight(.semibold).monospacedDigit())
                                    .foregroundStyle(.white)
                                    .padding(.horizontal, 7)
                                    .padding(.vertical, 2)
                                    .background(Capsule().fill(Color.black.opacity(0.55)))
                                    .overlay(Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
                                    .accessibilityIdentifier("story-gallery-counter")
                            }
                        }
                        .padding(.horizontal, 16)
                        .padding(.top, 8)
                        Spacer()
                        Text(shown.caption?.isEmpty == false ? shown.caption! : shown.label)
                            .font(.footnote)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 16)
                            .padding(.bottom, bottom + 8)
                    }
                    .allowsHitTesting(false)
                }
            }
        case .surface:
            if let surface = card?.surface {
                scroller { PlanCardSurfaceView(surface: surface, handle: handle, editable: false, bare: true) }
            }
        case .title:
            Color.clear
        }
    }

    private func scroller<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        ScrollView {
            content()
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 16)
                .padding(.top, top + 8)
                .padding(.bottom, bottom + 16)
                .background(GeometryReader { proxy in
                    Color.clear.preference(key: StoryScrollTop.self, value: proxy.frame(in: .named("story-body")).minY)
                })
        }
        .coordinateSpace(name: "story-body")
        .onPreferenceChange(StoryScrollTop.self) { scrollAtTop = $0 >= -1 }
        .onAppear { scrollAtTop = true }
    }
}

private struct StoryScrollTop: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}

/// One picture through the instance's attachment loader (the plan card's
/// picture tiles' loader), FITTED; the muted placeholder while it loads.
/// `fill` = the viewer's frame: fitted on black with no rounding.
struct StoryPicture: View {
    let picture: Stories.Picture
    var fill = false
    /// A thumbnail: the picture FILLS its box (cropped).
    var cover = false

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var image: UIImage?

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: fill ? 0 : GlassTokens.groupRadius)
        ZStack {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .aspectRatio(contentMode: cover ? .fill : .fit)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .clipped()
                    .clipShape(shape)
            } else if fill {
                AppIcon(AppIcons.editorImage, size: 28)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
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
            if let tile = SpecimenPicture.image(picture.attachmentId) {
                image = tile
                return
            }
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

/// The story's answer footer: the question's option chips (+ the free text
/// when custom) or a reply field. A plan never decides in a story (#EXP-1493:
/// its cover's `Open plan`). Answers go through `sessions.answer`.
private struct StoryFooter: View {
    let story: Stories.Story
    let footer: Stories.Footer
    let card: TreeMessageCard?
    let onAnswer: (ThreadCardAnswer) async -> Bool

    @State private var draft = PlanCardDraft()
    @State private var reply = ""
    @State private var busy = false

    var body: some View {
        switch footer {
        case .choices: choices
        case .reply: replyField
        case .open, .none: EmptyView()
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
