import ExpCore
import ExpUI
import ImageIO
internal import ExponentialUIPrimitives
import SwiftUI
import UIKit
#if canImport(ExponentialUI)
import ExponentialUI
#endif

/// EXP-1586: THE picture viewer, phone form — every place a picture opens
/// full size (a Results / `sessions_show` tile, the plan lightbox, a
/// comment's image) opens THIS full-screen page, never Quick Look. The
/// picture sits in the SDK's `ZoomableImageView` (catalog/style.json
/// `zoomableImage`, the core's maths): it opens at fit (never upscaled; a
/// tall capture at fit-width from its top), pinch zooms about the fingers,
/// a double tap goes rest ↔ max(100%, 2× rest), one finger pans only along
/// an axis with room, a tap toggles THE zoom pill. A set steps with
/// `‹ i / n ›` and, only while the picture rests, a horizontal swipe (no
/// wrap); a step opens the next picture at rest. Closing = ✕, or a swipe
/// down while a picture that fits rests.
struct PictureViewerItem: Identifiable, Hashable, Sendable {
    /// The attachment id (the zoom's reset key).
    let id: String
    /// The attachment's relative URL (`/api/attachments/{id}`).
    let src: String
    /// The picture's accessible name (alt / label / filename).
    let label: String
}

/// What the viewer opens: a set of pictures and the one it starts at.
struct PictureViewerSet: Identifiable {
    let items: [PictureViewerItem]
    let start: Int

    var id: String { items.indices.contains(start) ? items[start].id : "" }

    init(items: [PictureViewerItem], start: Int = 0) {
        self.items = items
        self.start = min(max(0, start), max(0, items.count - 1))
    }
}

extension View {
    /// Presents THE picture viewer over `opened` (nil = closed).
    func pictureViewer(_ opened: Binding<PictureViewerSet?>) -> some View {
        fullScreenCover(item: opened) { pictures in
            PictureViewer(pictures: pictures) { opened.wrappedValue = nil }
        }
    }
}

struct PictureViewer: View {
    let pictures: PictureViewerSet
    let onClose: () -> Void

    @Environment(\.motion) private var motion
    @State private var index = 0
    /// The shown picture is off its rest view (the set does not step).
    @State private var zoomed = false
    /// A pinch touched the page during the current gesture: its finger
    /// travel is never a swipe.
    @State private var pinchSeen = false
    /// The shown picture measured tall (w/h < 1/3): its rest view pans
    /// vertically, so a downward drag is its pan, never a close.
    @State private var tall = false
    @State private var forward = true
    @State private var dragDown: CGFloat = 0

    /// A horizontal swipe this far (pt) steps the set (web `SWIPE_PX`).
    private static let swipe: CGFloat = 48
    /// A downward swipe this far (pt) closes the viewer.
    private static let dismiss: CGFloat = 120

    private var item: PictureViewerItem? {
        pictures.items.indices.contains(index) ? pictures.items[index] : nil
    }

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                ZStack {
                    if let item {
                        PictureViewerPage(item: item, onZoomed: { zoomed = $0 }, onTall: { tall = $0 })
                            .id(item.id)
                            .transition(.asymmetric(
                                insertion: .move(edge: forward ? .trailing : .leading),
                                removal: .move(edge: forward ? .leading : .trailing)
                            ))
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .clipped()
                .offset(y: max(0, dragDown))
                .simultaneousGesture(swipes)
                .simultaneousGesture(
                    MagnifyGesture()
                        .onChanged { _ in pinchSeen = true }
                        .onEnded { _ in
                            Task {
                                try? await Task.sleep(for: .milliseconds(300))
                                pinchSeen = false
                            }
                        }
                )
            }
        }
        .preferredColorScheme(.dark)
        .onAppear { index = pictures.start }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("picture-viewer")
    }

    // MARK: - Chrome

    private var header: some View {
        ZStack {
            if pictures.items.count > 1 {
                HStack(spacing: 4) {
                    GhostIconButton(
                        AppIcons.uiChevronLeft,
                        accessibilityLabel: "Previous picture",
                        tint: .white,
                        enabled: index > 0
                    ) { step(-1) }
                    .accessibilityIdentifier("picture-viewer-prev")
                    Text("\(index + 1) / \(pictures.items.count)")
                        .font(.subheadline)
                        .monospacedDigit()
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .accessibilityIdentifier("picture-viewer-index")
                    GhostIconButton(
                        AppIcons.uiChevronRight,
                        accessibilityLabel: "Next picture",
                        tint: .white,
                        enabled: index < pictures.items.count - 1
                    ) { step(1) }
                    .accessibilityIdentifier("picture-viewer-next")
                }
            }
            HStack {
                GhostIconButton(AppIcons.uiClose, accessibilityLabel: "Close", tint: .white, action: onClose)
                    .accessibilityIdentifier("picture-viewer-close")
                Spacer(minLength: 0)
            }
        }
        .padding(.horizontal, 8)
        .frame(height: 52)
    }

    // MARK: - Swipes

    /// At rest only: a horizontal swipe steps the set, a downward one on a
    /// picture that fits closes. Zoomed, every drag is the picture's pan.
    private var swipes: some Gesture {
        DragGesture(minimumDistance: 12)
            .onChanged { value in
                guard !zoomed, !pinchSeen, !tall else { return }
                let t = value.translation
                if t.height > 0, abs(t.height) > abs(t.width) { dragDown = t.height }
            }
            .onEnded { value in
                defer {
                    pinchSeen = false
                    if dragDown != 0 { withAnimation(motion.fast) { dragDown = 0 } }
                }
                guard !zoomed, !pinchSeen else { return }
                let t = value.translation
                if abs(t.width) > abs(t.height) {
                    if t.width > Self.swipe { step(-1) } else if -t.width > Self.swipe { step(1) }
                } else if !tall, t.height > Self.dismiss {
                    onClose()
                }
            }
    }

    private func step(_ delta: Int) {
        let next = index + delta
        guard pictures.items.indices.contains(next) else { return }
        forward = delta > 0
        zoomed = false
        tall = false
        withAnimation(motion.standard) { index = next }
    }
}

/// One picture of the viewer: the bytes decoded (bounded, see
/// `PictureViewerDecode`), then laid out at the SOURCE's pixel size inside
/// `ZoomableImageView` as horizontal strips under Core Animation's texture
/// cap, so a 25k px full-page capture zooms like any screenshot.
private struct PictureViewerPage: View {
    let item: PictureViewerItem
    let onZoomed: (Bool) -> Void
    let onTall: (Bool) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.displayScale) private var displayScale

    private enum Phase {
        case loading
        case ready(PictureViewerDecode)
        case failed
    }

    @State private var phase: Phase = .loading

    var body: some View {
        Group {
            switch phase {
            case .loading:
                ProgressView()
                    .controlSize(.regular)
                    .tint(.white)
            case .failed:
                Text("Image unavailable")
                    .font(.callout)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            case let .ready(picture):
                zoomable(picture)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task(id: item.id) { await load() }
    }

    @ViewBuilder
    private func zoomable(_ picture: PictureViewerDecode) -> some View {
        #if canImport(ExponentialUI)
        ZoomableImageView(
            naturalSize: picture.naturalSize,
            maths: .core,
            resetID: item.id,
            label: item.label,
            pillStyle: Self.pillStyle,
            onZoom: { onZoomed(!$0.atFit) }
        ) {
            PictureViewerStrips(picture: picture)
        }
        #else
        // No eUI core in this build (no FFI xcframework): the picture fits.
        PictureViewerStrips(picture: picture)
            .aspectRatio(picture.naturalSize, contentMode: .fit)
            .accessibilityLabel(item.label)
        #endif
    }

    /// THE pill in the app's glass tokens.
    private static var pillStyle: ZoomPillStyle {
        .standard(PrimitiveTokens(
            foreground: .white,
            mutedForeground: .white.opacity(TextOpacity.secondary),
            card: GlassTokens.opaqueCardFill,
            border: GlassTokens.strokeCard,
            accent: GlassTokens.fillActive,
            hairline: GlassTokens.hairline,
            radiusSm: DesignTokens.Radius.sm,
            radiusMd: DesignTokens.Radius.md,
            radiusLg: DesignTokens.Radius.lg
        ))
    }

    private func load() async {
        phase = .loading
        onZoomed(false)
        if let tile = SpecimenPicture.image(item.src), let decoded = PictureViewerDecode(image: tile) {
            phase = .ready(decoded)
            return
        }
        let loader = AttachmentImageLoader(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            pendingImages: [:]
        )
        // Never below what the screen needs at fit-width (a tall capture's
        // rest view), so the rest view is always pixel-sharp.
        let screenPixels = Int((UIScreen.main.bounds.width * displayScale).rounded(.up))
        guard let data = try? await loader.data(item.src),
              let decoded = await Task.detached(priority: .userInitiated, operation: {
                  PictureViewerDecode.decode(data, minPixelWidth: screenPixels)
              }).value
        else {
            if !Task.isCancelled { phase = .failed }
            return
        }
        guard !Task.isCancelled else { return }
        onTall(decoded.naturalSize.width / decoded.naturalSize.height < sessionResultTallAspect)
        phase = .ready(decoded)
    }
}

/// The decoded strips drawn over the natural-size box (each strip its exact
/// share of the height, so the seams line up).
private struct PictureViewerStrips: View {
    let picture: PictureViewerDecode

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(picture.strips.enumerated()), id: \.offset) { _, strip in
                Image(uiImage: strip)
                    .resizable()
                    .interpolation(.high)
                    .frame(
                        width: picture.naturalSize.width,
                        height: CGFloat(strip.cgImage?.height ?? 0) * picture.naturalSize.height / picture.pixelHeight
                    )
            }
        }
    }
}

/// A viewer picture: the source's (oriented) pixel size = its 100% size in
/// points, and the decoded bitmap cut into `tallImageStripRanges` strips.
///
/// MEMORY: the source decodes at FULL resolution up to `pixelBudget`
/// (24 MP ≈ 96 MB RGBA: a 1440 × 16 000 full-page capture, any phone or
/// desktop screenshot), so every zoom up to 100% stays pixel-exact and
/// beyond it only magnifies source pixels. A larger source is downsampled
/// to the budget, but never narrower than the screen needs at fit-width
/// (the old tall viewer's decode), and no side past `maxSide` (the texture
/// cap for a strip's width). One picture is decoded at a time: a step
/// replaces it.
struct PictureViewerDecode: @unchecked Sendable {
    let strips: [UIImage]
    let naturalSize: CGSize
    /// The decoded bitmap's height (the strips' total rows).
    let pixelHeight: CGFloat

    static let pixelBudget = 24_000_000.0
    static let maxSide = 16_384.0

    init?(image: UIImage) {
        guard let cgImage = image.cgImage, cgImage.width > 0, cgImage.height > 0 else { return nil }
        strips = [image]
        naturalSize = CGSize(width: image.size.width, height: image.size.height)
        pixelHeight = CGFloat(cgImage.height)
    }

    private init(strips: [UIImage], naturalSize: CGSize, pixelHeight: CGFloat) {
        self.strips = strips
        self.naturalSize = naturalSize
        self.pixelHeight = pixelHeight
    }

    static func decode(_ data: Data, minPixelWidth: Int) -> PictureViewerDecode? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let rawWidth = properties[kCGImagePropertyPixelWidth] as? Int,
              let rawHeight = properties[kCGImagePropertyPixelHeight] as? Int,
              rawWidth > 0, rawHeight > 0
        else { return nil }
        // EXIF orientations 5…8 swap the axes.
        let orientation = properties[kCGImagePropertyOrientation] as? Int ?? 1
        let swapped = (5...8).contains(orientation)
        let width = Double(swapped ? rawHeight : rawWidth)
        let height = Double(swapped ? rawWidth : rawHeight)
        let budget = (pixelBudget / (width * height)).squareRoot()
        let floor = Double(minPixelWidth) / width
        let factor = min(1, maxSide / width, max(budget, floor))
        let maxPixel = Int((max(width, height) * factor).rounded(.up))
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixel,
        ]
        guard let cgImage = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary),
              cgImage.width > 0, cgImage.height > 0
        else { return nil }
        let strips = tallImageStripRanges(height: cgImage.height).compactMap { range in
            cgImage
                .cropping(to: CGRect(x: 0, y: range.y, width: cgImage.width, height: range.rows))
                .map { UIImage(cgImage: $0) }
        }
        guard !strips.isEmpty else { return nil }
        return PictureViewerDecode(
            strips: strips,
            naturalSize: CGSize(width: width, height: height),
            pixelHeight: CGFloat(cgImage.height)
        )
    }
}
