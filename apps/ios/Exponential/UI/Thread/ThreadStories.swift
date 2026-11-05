import ExpCore
import ExpUI
import SwiftUI
import UIKit

// The `story-tab` styleguide specimen's painter (fixtures/stories.json STORIES
// TAB; capture only). #FEED-191: the thread screen's last page is
// `ThreadResultsPage`; this goes with the specimen in the final cleanup.

/// The waiting tint (ask-dock: plan primary, question yellow).
private func storyTint(_ story: Stories.Story) -> Color {
    (story.kind == .question ? Dock.Tint.question : Dock.Tint.plan).color
}

/// How a story's body draws in place (absent = the line alone).
struct StoryBodyRenderers {
    /// The story's raw card (its surface), nil = none.
    var card: (String) -> TreeMessageCard? = { _ in nil }
    /// A surface press's reply text (THE chat's `surfaceReply`), sent to the
    /// story's session; nil = the surface takes no input.
    var onSurfaceReply: ((Stories.Story, String) -> Void)?
}

/// THE Stories tab body: `lead` (the PR block), then the stories INLINE in
/// fold order `tile.listGap` apart, else THE EmptyState. A pure painter: the
/// styleguide specimen draws it over `tabCases[1]`.
struct StoryTabView: View {
    let ring: Stories.StoryRing?
    var lead: AnyView?
    var renderers = StoryBodyRenderers()
    /// A tap on a story's line (picture 0) or a picture (its index).
    var onOpen: (Stories.Story, Int) -> Void = { _, _ in }
    /// #EXP-1493: a tap anywhere on a plan section = THE plan page.
    var onOpenPlan: (Stories.Story) -> Void = { _ in }

    var body: some View {
        let stories = ring?.stories ?? []
        VStack(alignment: .leading, spacing: CGFloat(Stories.Tab.Tile.leadGap)) {
            if let lead { lead }
            if stories.isEmpty {
                EmptyState(title: Stories.Tab.Copy.empty, icon: AppIcons.threadStories, size: .page)
                    .frame(maxWidth: .infinity)
                    .padding(.top, 48)
            } else {
                VStack(alignment: .leading, spacing: CGFloat(Stories.Tab.Tile.listGap)) {
                    ForEach(stories) { story in
                        StoryTile(story: story, renderers: renderers, onOpen: onOpen, onOpenPlan: onOpenPlan)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Stories.Tab.TestIds.list)
    }
}

/// One story's SECTION (#EXP-1462): line 1 = the title (one line; none = the
/// kind word) with `meta` muted at the end (a waiting one leads with the
/// tint dot and tints its meta); the description (muted, wrapping)
/// `tile.textGap` under it; then its body `tile.gap` under that, by the
/// frame rule: a PLAN (#EXP-1493) = its counts line (after the description)
/// then its pictures row,
/// a tap anywhere opening THE plan page; else ONE sideways row of pictures
/// (a muted caption-or-label line under each), else THE surface card inline
/// (no title over it, presses reply like the chat), else nothing. THE kind
/// badge leads line 1.
struct StoryTile: View {
    let story: Stories.Story
    var renderers = StoryBodyRenderers()
    let onOpen: (Stories.Story, Int) -> Void
    var onOpenPlan: (Stories.Story) -> Void = { _ in }

    @State private var handle = PlanCardSurfaceHandle()

    var body: some View {
        let gap = CGFloat(Stories.Tab.Tile.gap)
        // The frame rule: a plan = counts + pictures; pictures win, else the surface.
        let plan = story.kind == .plan
        let surface = !plan && story.pictures.isEmpty && story.surface ? renderers.card(story.id)?.surface : nil
        let counts = story.counts.map(PlanPage.countsLine) ?? ""
        VStack(alignment: .leading, spacing: gap) {
            Button { plan ? onOpenPlan(story) : onOpen(story, 0) } label: {
                VStack(alignment: .leading, spacing: CGFloat(Stories.Tab.Tile.textGap)) {
                    HStack(spacing: 8) {
                        StoryKindBadge(kind: story.kind)
                        if story.waiting {
                            Circle()
                                .fill(storyTint(story))
                                .frame(width: 6, height: 6)
                                .accessibilityHidden(true)
                        }
                        Text(story.title ?? Stories.Copy.kind(story.kind))
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Text(story.meta)
                            .font(.system(size: 12))
                            .foregroundStyle(story.waiting ? storyTint(story) : DesignTokens.Palette.mutedForeground)
                            .lineLimit(1)
                            .fixedSize()
                    }
                    if let description = story.description, !description.isEmpty {
                        Text(description)
                            .font(.system(size: 14))
                            .foregroundStyle(DesignTokens.Palette.mutedForeground)
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .multilineTextAlignment(.leading)
                    }
                    if plan, !counts.isEmpty {
                        Text(counts)
                            .font(.system(size: 12))
                            .foregroundStyle(DesignTokens.Palette.mutedForeground)
                            .lineLimit(1)
                            .accessibilityIdentifier("story-tile-counts")
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if !story.pictures.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(alignment: .top, spacing: gap) {
                        ForEach(Array(story.pictures.enumerated()), id: \.offset) { index, picture in
                            Button { plan ? onOpenPlan(story) : onOpen(story, index) } label: {
                                StoryThumbnail(picture: picture)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
                .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
            } else if let surface {
                let reply = renderers.onSurfaceReply.map { send in { (text: String) in send(story, text) } }
                SurfaceCardFrame(title: nil, animate: false) {
                    PlanCardSurfaceView(
                        surface: surface, handle: handle, editable: reply != nil, onReply: reply, bare: true
                    )
                }
                .accessibilityIdentifier("story-tile-surface")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Stories.Tab.TestIds.tile)
    }
}

/// One thumbnail: `tile.thumbHeight` tall, width by aspect (unknown = 4:3)
/// capped at `tile.thumbMaxWidth`, `tile.radius`, the glass-stroke hairline
/// on the muted fill, cropped from the top; the caption-else-label line 6
/// under it.
struct StoryThumbnail: View {
    let picture: Stories.Picture

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var image: UIImage?

    var body: some View {
        let width = CGFloat(Stories.Tab.Tile.thumbWidth(width: picture.width, height: picture.height))
        let height = CGFloat(Stories.Tab.Tile.thumbHeight)
        let shape = RoundedRectangle(cornerRadius: CGFloat(Stories.Tab.Tile.radius), style: .continuous)
        VStack(alignment: .leading, spacing: 6) {
            ZStack(alignment: .top) {
                DesignTokens.Palette.muted
                if let image {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFill()
                        .frame(width: width, height: height, alignment: .top)
                }
            }
            .frame(width: width, height: height, alignment: .top)
            .clipShape(shape)
            .overlay(shape.strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
            Text(picture.caption ?? picture.label)
                .font(.system(size: 12))
                .foregroundStyle(DesignTokens.Palette.mutedForeground)
                .lineLimit(1)
                .truncationMode(.tail)
        }
        .frame(width: width, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(picture.caption ?? picture.label)
        .accessibilityAddTraits(.isButton)
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
}
