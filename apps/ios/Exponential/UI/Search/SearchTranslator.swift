import ExpCore
import Foundation
#if canImport(FoundationModels)
import FoundationModels
#endif

/// #NEW-136: the model behind the search's `Translate to a search` row on
/// iOS: Apple Foundation Models, on the device. Nothing leaves the phone;
/// there is no server call and no other model.
///
/// Available only on iOS 26+ with Apple Intelligence on and the model ready
/// (`SystemLanguageModel.default.availability`). Anything else (older iOS,
/// an unsupported device, Apple Intelligence off, the model still
/// downloading) = not available = the search shows no row.
struct FoundationModelsSearchTranslator: IssueSearchTranslating {
    struct Unavailable: Error {}

    var isAvailable: Bool {
        #if canImport(FoundationModels)
        if #available(iOS 26.0, *) {
            return SystemLanguageModel.default.availability == .available
        }
        #endif
        return false
    }

    func answer(prompt: String, instructions: String) async throws -> String {
        #if canImport(FoundationModels)
        if #available(iOS 26.0, *) {
            // A fresh session per call: no tools, no history, one short line.
            let session = LanguageModelSession(instructions: instructions)
            let options = GenerationOptions(temperature: 0, maximumResponseTokens: 80)
            return try await session.respond(to: prompt, options: options).content
        }
        #endif
        throw Unavailable()
    }
}

enum SearchTranslator {
    /// The translator the search uses: the on-device model. DEBUG captures
    /// (`-uiTesting`, never a release build) may stand a fixed answer in.
    static func make() -> any IssueSearchTranslating {
        #if DEBUG
        if let standIn = UITestingSession.searchTranslator { return standIn }
        #endif
        return FoundationModelsSearchTranslator()
    }
}
