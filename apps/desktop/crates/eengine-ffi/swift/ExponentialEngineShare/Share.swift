// #VAPP-265, #VAPP-268: `share`. Its own module so an iOS app links
// LinkPresentation and the share sheet only when it declares the capability (ITMS-90683).
import Foundation
import ExponentialEngine
import ExponentialEngineCapabilityCore

/// What share.send hands the sheet.
public struct ShareItems: Equatable {
    public var title: String?
    public var text: String?
    public var url: URL?
    public var file: PickedFile?
    public init(title: String?, text: String?, url: URL?, file: PickedFile?) {
        self.title = title; self.text = text; self.url = url; self.file = file
    }
}

public protocol ShareSheet: AnyObject {
    /// The platform's share sheet; false = dismissed.
    func share(_ items: ShareItems) throws -> Bool
}

/// `share`: send (the share sheet).
public final class ShareCapability: Capability, @unchecked Sendable {
    let sheet: ShareSheet?
    let runner: Runner

    public init(sheet: ShareSheet?, isMainThread: @escaping () -> Bool = { Thread.isMainThread }) {
        self.sheet = sheet
        runner = Runner(capability: "share", isMainThread: isMainThread)
    }

    public func name() -> String { "share" }
    public func toolsJson() -> String { ToolCatalog.json("share") }

    public func call(tool: String, argumentsJson: String) throws -> String {
        runner.run(tool, argumentsJson) { tool, args in
            guard tool == "send" else { throw runner.unknown(tool) }
            guard let sheet else { throw CapabilityFailure.unavailable }
            let url = try args.optionalString("url").map { text -> URL in
                guard let url = URL(string: text), url.scheme != nil else { throw CapabilityFailure.failed("arguments: url is not a URL") }
                return url
            }
            var file = try args.file("file")
            if let named = file { file?.name = FileNames.safe(named.name) }
            let items = ShareItems(title: try args.optionalString("title"), text: try args.optionalString("text"), url: url, file: file)
            guard items.text != nil || items.url != nil || items.file != nil else { throw CapabilityFailure.failed("nothing to share") }
            return ["shared": try sheet.share(items)]
        }
    }
}
