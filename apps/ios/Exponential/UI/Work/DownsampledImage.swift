import ExpCore
import ImageIO
import SwiftUI
import UIKit

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
