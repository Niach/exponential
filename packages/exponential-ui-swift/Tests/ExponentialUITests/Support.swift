import Foundation
import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// The shared fixtures of `packages/exponential-ui/fixtures` (in-repo path
/// from this file; the suite skips when the checkout lacks them).
enum Fixtures {
    static let dir: URL = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().appendingPathComponent("exponential-ui/fixtures")

    static func available() -> Bool { FileManager.default.fileExists(atPath: dir.path) }

    static func json(_ name: String) throws -> JSONValue {
        let data = try Data(contentsOf: dir.appendingPathComponent(name))
        return JSONValue(any: try JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]))
    }

    static func text(_ name: String) throws -> String {
        try String(contentsOf: dir.appendingPathComponent(name), encoding: .utf8)
    }
}

@MainActor
func makeModel(_ id: String = "t", theme: String? = "exponential", mode: Mode = .dark, width: CGFloat = 390, fixed: Bool = false) throws -> SurfaceModel {
    var options = SurfaceOptions()
    options.theme = theme.flatMap { ThemeHandle.builtin($0) }
    options.mode = mode
    let m = try SurfaceModel(id: id, options: options, host: NoHost())
    m.fixedMeasure = fixed
    m.setViewport(width: width, height: 800)
    return m
}

/// A host that records what it receives.
@MainActor
final class RecordingHost: HostPlugin {
    var actions: [SurfaceActionEvent] = []
    var inputs: [SurfaceInputEvent] = []
    var urls: [String] = []
    var unknowns: [String] = []
    var uploads: [SurfaceUploadEvent] = []
    func onAction(_ event: SurfaceActionEvent) { actions.append(event) }
    func onInput(_ event: SurfaceInputEvent) { inputs.append(event) }
    func openUrl(_ url: String) { urls.append(url) }
    func onUnknown(component: String, catalogId: String?, id: String) { unknowns.append(component) }
    func onUpload(_ event: SurfaceUploadEvent) { uploads.append(event) }
}
