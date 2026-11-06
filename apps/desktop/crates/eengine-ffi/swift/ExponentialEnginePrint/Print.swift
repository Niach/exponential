// #VAPP-265, #VAPP-268: `print`. Its own module so an iOS app links
// the print dialog only when it declares the capability (ITMS-90683).
import Foundation
import ExponentialEngine
import ExponentialEngineCapabilityCore

public protocol PrintDialog: AnyObject {
    /// The platform's print dialog for a PDF, PNG or JPEG; false = cancelled.
    func print(_ file: PickedFile) throws -> Bool
}

/// `print`: print (the print dialog; `print.pdf` is the Engine's own).
public final class PrintCapability: Capability, @unchecked Sendable {
    let dialog: PrintDialog?
    let runner: Runner

    public init(dialog: PrintDialog?, isMainThread: @escaping () -> Bool = { Thread.isMainThread }) {
        self.dialog = dialog
        runner = Runner(capability: "print", isMainThread: isMainThread)
    }

    public func name() -> String { "print" }
    public func toolsJson() -> String { ToolCatalog.json("print") }

    public func call(tool: String, argumentsJson: String) throws -> String {
        runner.run(tool, argumentsJson) { tool, args in
            guard tool == "print" else { throw runner.unknown(tool) }
            guard let dialog else { throw CapabilityFailure.unavailable }
            let file = PickedFile(name: FileNames.safe(try args.string("name")), mimeType: try args.string("mimeType"), data: try args.bytes("data"))
            return ["printed": try dialog.print(file)]
        }
    }
}
