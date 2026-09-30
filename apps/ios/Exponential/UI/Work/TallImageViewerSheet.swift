import ExpCore
import ExpUI
import ImageIO
import SwiftUI
import UIKit

/// EXP-1128: the viewer a TALL Results picture (`sessionResultIsTall`, a
/// full-page capture) opens instead of Quick Look, which fits the whole page
/// into the screen and shows another sliver. The picture renders at the
/// sheet's width (never wider than its natural point size) in a vertical
/// scroll, drawn as horizontal strips of `tallImageStripRanges` rows so no one
/// layer outgrows Core Animation's texture cap (a 25k px capture would draw
/// blank as a single image). Mirrors web/desktop/Android's tall viewer. The
/// bytes decode through ImageIO no wider than the sheet needs, off the main
/// thread (`DownsampledImage`): a 4000×60000 capture decoded whole is ~960 MB.
struct TallImageViewerSheet: View {
    let entry: SessionResultEntry

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.displayScale) private var displayScale

    private enum Phase {
        case loading
        case ready(strips: [UIImage], naturalWidth: CGFloat, pixelWidth: CGFloat)
        case failed
    }

    @State private var phase: Phase = .loading
    @State private var sheetWidth: CGFloat = 0

    var body: some View {
        GlassSheetChrome(title: entry.label, height: .full) {
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { width in
                    sheetWidth = width
                }
        }
        .accessibilityIdentifier("tall-image-viewer")
        .task(id: entry.attachmentId) { await load() }
    }

    @ViewBuilder
    private var content: some View {
        switch phase {
        case .loading:
            ProgressView()
                .controlSize(.small)
                .tint(.white)
                .padding(.vertical, 24)
        case .failed:
            Text("Image unavailable")
                .font(.callout)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .padding(.vertical, 24)
        case let .ready(strips, naturalWidth, pixelWidth):
            let width = max(1, min(sheetWidth, naturalWidth))
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(Array(strips.enumerated()), id: \.offset) { _, strip in
                        Image(uiImage: strip)
                            .resizable()
                            // Each strip's exact share of the scaled height,
                            // so the seams line up.
                            .frame(
                                width: width,
                                height: CGFloat(strip.cgImage?.height ?? 0) * width / pixelWidth
                            )
                    }
                }
                .frame(maxWidth: .infinity)
                .padding(.bottom, 16)
            }
            .accessibilityLabel("\(entry.topic), \(entry.label), tall")
        }
    }

    private func load() async {
        phase = .loading
        let loader = AttachmentImageLoader(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            pendingImages: [:]
        )
        // The geometry may not have landed yet at first load: the screen's
        // width is never narrower than the sheet, so it only over-decodes.
        let pointWidth = sheetWidth > 0 ? sheetWidth : UIScreen.main.bounds.width
        let maxPixelWidth = Int((pointWidth * displayScale).rounded(.up))
        guard let data = try? await loader.data(entry.url),
              let decoded = await Task.detached(priority: .userInitiated, operation: {
                  DownsampledImage.decode(data, maxPixelWidth: maxPixelWidth)
              }).value,
              let cgImage = decoded.image.cgImage,
              cgImage.width > 0
        else {
            if !Task.isCancelled { phase = .failed }
            return
        }
        let strips = tallImageStripRanges(height: cgImage.height).compactMap { range in
            cgImage
                .cropping(to: CGRect(x: 0, y: range.y, width: cgImage.width, height: range.rows))
                .map { UIImage(cgImage: $0) }
        }
        guard !strips.isEmpty else {
            phase = .failed
            return
        }
        phase = .ready(
            strips: strips,
            // The SOURCE's point size (1 px = 1 pt, like `UIImage(data:)`),
            // so a picture narrower than the sheet keeps its natural width.
            naturalWidth: CGFloat(decoded.sourcePixelWidth),
            pixelWidth: CGFloat(cgImage.width)
        )
    }
}

/// EXP-1128: a picture decoded through ImageIO no wider than `maxPixelWidth`
/// and never upscaled. `kCGImageSourceThumbnailMaxPixelSize` caps the LARGER
/// side, so a tall picture's cap is derived from its aspect. Runs wherever it
/// is called: callers keep it off the main thread.
struct DownsampledImage: Sendable {
    let image: UIImage
    /// The source's pixel width, before downsampling.
    let sourcePixelWidth: Int

    static func decode(_ data: Data, maxPixelWidth: Int) -> DownsampledImage? {
        guard maxPixelWidth > 0,
              let source = CGImageSourceCreateWithData(data as CFData, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int,
              width > 0, height > 0
        else { return nil }
        let factor = min(1, Double(maxPixelWidth) / Double(width))
        let maxSide = Int((Double(max(width, height)) * factor).rounded(.up))
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: maxSide,
        ]
        guard let cgImage = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
        else { return nil }
        return DownsampledImage(image: UIImage(cgImage: cgImage), sourcePixelWidth: width)
    }

    /// The tile's picture: the top 4:3 crop COPIED into its own bitmap, so
    /// caching it does not pin the whole downsampled page.
    func tallTopTile() -> UIImage? {
        guard let crop = sessionResultTallTopCrop(image) else { return nil }
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        return UIGraphicsImageRenderer(size: crop.size, format: format).image { _ in
            crop.draw(at: .zero)
        }
    }
}

/// EXP-1128: a tall picture's TOP at the tile's 4:3 aspect — a `CGImage`
/// crop (no pixel copy) that keeps the thumbnail under the texture cap. nil
/// when the image has no bitmap to crop.
func sessionResultTallTopCrop(_ image: UIImage) -> UIImage? {
    guard let cgImage = image.cgImage else { return nil }
    let width = cgImage.width
    let height = min(cgImage.height, Int((Double(width) * 3 / 4).rounded()))
    guard width > 0, height > 0,
          let top = cgImage.cropping(to: CGRect(x: 0, y: 0, width: width, height: height))
    else { return nil }
    return UIImage(cgImage: top, scale: image.scale, orientation: image.imageOrientation)
}
