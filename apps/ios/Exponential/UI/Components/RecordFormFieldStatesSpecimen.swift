import ExponentialUI
import ExponentialUICore
import SwiftUI

/// Round 9 §6.6 (`sg_record-form-field-states`): the `record-form` entry's
/// field states (web `ExponentialUiFieldStates`, the KitchenSink's
/// `fieldStates`, Android's `RecordFormFieldStatesSpecimenScreen`): one Input
/// per check severity with its message showing: error = the invalid state,
/// warning and info advisory. Each field blurs once, so the core reveals its
/// check. Capture only.
struct RecordFormFieldStatesSpecimen: View {
    private static let fields = [("email", "Email", "ada@", "/error"), ("password", "Password", "hunter2", "/warning"), ("nickname", "Nickname", "ada", "/info")]

    // Built up front: an `.onAppear` on the empty Group never fires.
    @State private var model: SurfaceModel? = Self.make()

    private static func failing(_ severity: String, _ message: String) -> JSONValue {
        .object(["valid": .bool(false), "severity": .string(severity), "message": .string(message)])
    }

    private static func make() -> SurfaceModel? {
        let inputs = fields.map { id, label, value, path in
            #"{"id":"\#(id)","component":"Input","props":{"label":"\#(label)","name":"\#(id)","value":"\#(value)","validateOn":"blur","checks":[{"condition":{"@path":"\#(path)"}}]}}"#
        }
        let json = #"{"id":"field-states","component":"Stack","props":{"gap":"md"},"children":[\#(inputs.joined(separator: ","))]}"#
        guard let m = try? SurfaceModel(id: "sg-field-states", options: SurfaceOptions(theme: ThemeHandle.builtin("exponential"), mode: .dark), host: NoHost()) else { return nil }
        try? m.setNested(json: json)
        m.setData(path: "/error", value: failing("error", "Not an email"))
        m.setData(path: "/warning", value: failing("warning", "Weak: add a few more characters"))
        m.setData(path: "/info", value: failing("info", "Shown on your profile"))
        return m
    }

    var body: some View {
        Group {
            if let model {
                ExponentialSurface(model: model)
                    .task {
                        try? await Task.sleep(for: .milliseconds(300))
                        for (id, _, _, _) in Self.fields { model.fire(id: "\(id).field", "blur") }
                    }
            }
        }
        .padding(.horizontal, 16)
        .accessibilityIdentifier("record-form-field-states-specimen")
    }
}
