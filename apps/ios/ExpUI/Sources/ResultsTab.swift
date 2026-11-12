import ExpCore
import SwiftUI

// #FEED-191 RESULTS TAB ×iOS (fixtures/results-tab.json; web
// `packages/ui/src/results-tab.tsx`): the session screen's last tab, phone form.
// The model is ExpCore's `ResultsTab.results`; these only paint it. The host
// hands in the PR block (`lead`), a picture's bytes and a surface's renderer.

public enum ResultsTabMetrics {
    /// A thumbnail's width: by the picture's aspect at `tile.thumbHeight`,
    /// capped at `tile.thumbMaxWidth`; an unknown size = 4:3.
    public static func thumbWidth(_ picture: ResultsTab.Picture) -> CGFloat {
        var aspect = 4.0 / 3.0
        if let width = picture.width, let height = picture.height, width > 0, height > 0 {
            aspect = width / height
        }
        let tile = (ResultsTab.Tab.Tile.thumbHeight, ResultsTab.Tab.Tile.thumbMaxWidth)
        return CGFloat(min(tile.1, (tile.0 * aspect).rounded()))
    }

    /// The muted line under a thumbnail, and its accessible name.
    public static func pictureText(_ picture: ResultsTab.Picture) -> String {
        picture.caption ?? picture.label
    }
}

/// THE Results tab body: `lead` (the PR block), then the results INLINE
/// `tile.listGap` apart, else THE EmptyState.
public struct ResultsTabView<Lead: View, Thumbnail: View, Surface: View>: View {
    let results: [ResultsTab.Result]
    let onOpenPicture: (ResultsTab.Result, Int) -> Void
    let lead: Lead
    let thumbnail: (ResultsTab.Picture) -> Thumbnail
    let surface: (ResultsTab.Result) -> Surface

    public init(
        results: [ResultsTab.Result],
        onOpenPicture: @escaping (ResultsTab.Result, Int) -> Void,
        @ViewBuilder lead: () -> Lead,
        @ViewBuilder thumbnail: @escaping (ResultsTab.Picture) -> Thumbnail,
        @ViewBuilder surface: @escaping (ResultsTab.Result) -> Surface
    ) {
        self.results = results
        self.onOpenPicture = onOpenPicture
        self.lead = lead()
        self.thumbnail = thumbnail
        self.surface = surface
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: CGFloat(ResultsTab.Tab.Tile.leadGap)) {
            lead
            if results.isEmpty {
                EmptyState(title: ResultsTab.Tab.Copy.empty, icon: AppIcons.chatResults, size: .page)
                    .frame(maxWidth: .infinity)
                    .padding(.top, 48)
            } else {
                // Large chats: a long chat's results (100+, each a
                // surface or a picture row) are built as they scroll in.
                LazyVStack(alignment: .leading, spacing: CGFloat(ResultsTab.Tab.Tile.listGap)) {
                    ForEach(results) { result in
                        ResultSection(result: result, onOpenPicture: { onOpenPicture(result, $0) }, thumbnail: thumbnail) {
                            surface(result)
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(ResultsTab.Tab.TestIds.list)
    }
}

/// One SECTION: line 1 the title with the age muted at the trailing end, no
/// badge; the description (muted, wrapping) `tile.textGap` under it; then
/// `tile.gap` under that: pictures win (ONE sideways row of thumbnails, a muted
/// caption-or-label under each), else the surface. A press on a picture opens
/// the lightbox; nothing else presses.
struct ResultSection<Thumbnail: View, Surface: View>: View {
    let result: ResultsTab.Result
    let onOpenPicture: (Int) -> Void
    let thumbnail: (ResultsTab.Picture) -> Thumbnail
    @ViewBuilder let surface: () -> Surface

    var body: some View {
        let gap = CGFloat(ResultsTab.Tab.Tile.gap)
        VStack(alignment: .leading, spacing: gap) {
            VStack(alignment: .leading, spacing: CGFloat(ResultsTab.Tab.Tile.textGap)) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(result.title)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Text(result.age)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Palette.mutedForeground)
                        .fixedSize()
                }
                if let description = result.description {
                    Text(description)
                        .font(.subheadline)
                        .foregroundStyle(DesignTokens.Palette.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            if !result.pictures.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(alignment: .top, spacing: gap) {
                        ForEach(Array(result.pictures.enumerated()), id: \.offset) { index, picture in
                            Button { onOpenPicture(index) } label: {
                                ResultThumbnail(picture: picture) { thumbnail(picture) }
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
                .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
            } else if result.surface {
                surface()
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(ResultsTab.Tab.TestIds.section)
    }
}

/// One thumbnail: `tile.thumbHeight` tall, width by aspect, `tile.radius`, the
/// glass-stroke hairline on the muted fill, cropped from the top; the
/// caption-else-label line 6 under it.
struct ResultThumbnail<Bytes: View>: View {
    let picture: ResultsTab.Picture
    @ViewBuilder let image: () -> Bytes

    var body: some View {
        let width = ResultsTabMetrics.thumbWidth(picture)
        let height = CGFloat(ResultsTab.Tab.Tile.thumbHeight)
        let shape = RoundedRectangle(cornerRadius: CGFloat(ResultsTab.Tab.Tile.radius), style: .continuous)
        let text = ResultsTabMetrics.pictureText(picture)
        VStack(alignment: .leading, spacing: 6) {
            ZStack(alignment: .top) {
                DesignTokens.Palette.muted
                image()
                    .frame(width: width, height: height, alignment: .top)
            }
            .frame(width: width, height: height, alignment: .top)
            .clipShape(shape)
            .overlay(shape.strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
            Text(text)
                .font(.caption)
                .foregroundStyle(DesignTokens.Palette.mutedForeground)
                .lineLimit(1)
                .truncationMode(.tail)
        }
        .frame(width: width, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(text)
        .accessibilityAddTraits(.isButton)
    }
}
