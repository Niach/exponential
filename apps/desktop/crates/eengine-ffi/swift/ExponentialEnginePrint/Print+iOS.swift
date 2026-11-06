// #VAPP-265, #VAPP-268: the print seam on UIPrintInteractionController. Its own module so an iOS app links
// the print dialog only when it declares the capability (ITMS-90683).
#if os(iOS)
import Foundation
import UIKit
import ExponentialEngineCapabilityCore

/// UIPrintInteractionController with the bytes; `printed` = its completion's `completed`.
public final class SystemPrintDialog: PrintDialog, @unchecked Sendable {
    let config: CapabilityConfig
    public init(_ config: CapabilityConfig) { self.config = config }

    public func print(_ file: PickedFile) throws -> Bool {
        try Blocking.onMain { [config] finish in
            guard UIPrintInteractionController.isPrintingAvailable else { return finish(.failure(CapabilityFailure.unavailable)) }
            guard UIPrintInteractionController.canPrint(file.data) else { return finish(.failure(CapabilityFailure.failed("this file cannot be printed"))) }
            guard let top = config.presenter() else { return finish(.failure(CapabilityFailure.failed("no view controller to present from"))) }
            let controller = UIPrintInteractionController.shared
            let info = UIPrintInfo(dictionary: nil)
            info.jobName = file.name
            info.outputType = (file.mimeType ?? "").hasPrefix("image/") ? .photo : .general
            controller.printInfo = info
            controller.printingItem = file.data
            let completion: UIPrintInteractionController.CompletionHandler = { _, completed, error in
                finish(error.map { .failure($0) } ?? .success(completed))
            }
            let shown = top.traitCollection.userInterfaceIdiom == .pad
                ? controller.present(from: CGRect(x: top.view.bounds.midX, y: top.view.bounds.midY, width: 0, height: 0), in: top.view, animated: true, completionHandler: completion)
                : controller.present(animated: true, completionHandler: completion)
            if !shown { finish(.failure(CapabilityFailure.failed("the print dialog did not open"))) }
        }
    }
}
#endif
