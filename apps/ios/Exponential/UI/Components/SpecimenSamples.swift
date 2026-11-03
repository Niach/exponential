import Foundation
import UIKit

/// EXP-1654: the committed sample pictures (`packages/exponential-ui/samples`,
/// bundled as the `samples` folder). A styleguide specimen's attachment id
/// named in `samples.json` `attachments` (plan-page `sg-1`…, story-tab
/// `att-w`, `att-i`) paints that PNG, never fetching. Styleguide captures
/// only (`-uiTesting -uiTestingStyleguide <id>`): the product never reads it.
enum SpecimenSamples {
    private static let directory = Bundle.main.url(forResource: "samples", withExtension: nil)

    /// attachment id → file name; empty outside a styleguide capture.
    private static let attachments: [String: String] = {
        guard UITestingStyleguide.requested != nil,
              let url = directory?.appendingPathComponent("samples.json"),
              let data = try? Data(contentsOf: url),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return [:] }
        return root["attachments"] as? [String: String] ?? [:]
    }()

    private nonisolated(unsafe) static let cache = NSCache<NSString, UIImage>()

    /// The sample for an attachment id or its `/api/attachments/<id>` src.
    static func image(_ src: String) -> UIImage? {
        let prefix = "/api/attachments/"
        let id = src.hasPrefix(prefix) ? String(src.dropFirst(prefix.count)) : src
        guard let file = attachments[id], let url = directory?.appendingPathComponent(file) else { return nil }
        if let hit = cache.object(forKey: file as NSString) { return hit }
        guard let image = UIImage(contentsOfFile: url.path) else { return nil }
        cache.setObject(image, forKey: file as NSString)
        return image
    }
}
