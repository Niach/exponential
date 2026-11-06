// #VAPP-265, #VAPP-268: `files`. Its own module so an iOS app links
// the document picker only when it declares the capability (ITMS-90683).
import Foundation
import UniformTypeIdentifiers
import ExponentialEngine
import ExponentialEngineCapabilityCore

public protocol DocumentPicker: AnyObject {
    /// The platform's open dialog for `types`; [] = cancelled.
    func open(types: [UTType], multiple: Bool) throws -> [PickedFile]
    /// The platform's save (export) dialog; false = cancelled.
    func save(_ file: PickedFile) throws -> Bool
}

/// `files`: open (the open dialog), save (the save/export dialog).
public final class FilesCapability: Capability, @unchecked Sendable {
    let picker: DocumentPicker?
    let runner: Runner

    public init(picker: DocumentPicker?, isMainThread: @escaping () -> Bool = { Thread.isMainThread }) {
        self.picker = picker
        runner = Runner(capability: "files", isMainThread: isMainThread)
    }

    public func name() -> String { "files" }
    public func toolsJson() -> String { ToolCatalog.json("files") }

    public func call(tool: String, argumentsJson: String) throws -> String {
        runner.run(tool, argumentsJson) { tool, args in
            guard ["open", "save"].contains(tool) else { throw runner.unknown(tool) }
            guard let picker else { throw CapabilityFailure.unavailable }
            if tool == "open" {
                let types = AcceptedTypes.contentTypes(try args.strings("accept"))
                let multiple = try args.bool("multiple", default: false)
                let files = try picker.open(types: types, multiple: multiple).prefix(multiple ? 100 : 1)
                return ["files": files.map { file -> [String: Any] in
                    let name = FileNames.clamp(file.name, fallback: "file")
                    return ["name": name, "mimeType": file.mimeType.map { String($0.prefix(100)) } ?? MimeTypes.forName(name), "data": Base64.encode(file.data)]
                }]
            }
            let file = PickedFile(name: FileNames.safe(try args.string("name")), mimeType: try args.string("mimeType"), data: try args.bytes("data"))
            return ["saved": try picker.save(file)]
        }
    }
}
