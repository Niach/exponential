// #VAPP-265, #VAPP-268: the share seam on UIActivityViewController. Its own module so an iOS app links
// LinkPresentation and the share sheet only when it declares the capability (ITMS-90683).
#if os(iOS)
import Foundation
import LinkPresentation
import UIKit
import ExponentialEngineCapabilityCore

/// UIActivityViewController; `shared` = its completion's `completed`.
public final class SystemShareSheet: ShareSheet, @unchecked Sendable {
    let config: CapabilityConfig
    public init(_ config: CapabilityConfig) { self.config = config }

    public func share(_ items: ShareItems) throws -> Bool {
        try withTempFile(items.file) { fileURL in
            try Blocking.onMain { [config] finish in
                var activity: [Any] = []
                if let text = items.text { activity.append(text) }
                if let url = items.url { activity.append(url) }
                if let fileURL { activity.append(fileURL) }
                if let title = items.title, !activity.isEmpty { activity[0] = TitledItem(activity[0], title: title) }
                let controller = UIActivityViewController(activityItems: activity, applicationActivities: nil)
                controller.completionWithItemsHandler = { _, completed, _, error in
                    finish(error.map { .failure($0) } ?? .success(completed))
                }
                present(config, controller, finish)
            }
        }
    }
}

/// The share title: the subject (mail) and the sheet's header.
final class TitledItem: NSObject, UIActivityItemSource, @unchecked Sendable { // nonisolated: UIKit may ask off the main thread
    let item: Any
    let title: String
    init(_ item: Any, title: String) { self.item = item; self.title = title }

    func activityViewControllerPlaceholderItem(_ activityViewController: UIActivityViewController) -> Any { item }
    func activityViewController(_ activityViewController: UIActivityViewController, itemForActivityType activityType: UIActivity.ActivityType?) -> Any? { item }
    func activityViewController(_ activityViewController: UIActivityViewController, subjectForActivityType activityType: UIActivity.ActivityType?) -> String { title }
    func activityViewControllerLinkMetadata(_ activityViewController: UIActivityViewController) -> LPLinkMetadata? {
        let metadata = LPLinkMetadata()
        metadata.title = title
        if let url = item as? URL { metadata.originalURL = url; metadata.url = url }
        return metadata
    }
}
#endif
