import ExpCore
import SwiftUI

// The remote Start-coding options sheet (EXP-149) — the iOS twin of the
// desktop IDE's Start-coding dialog (single-issue mode): Model / Effort
// pickers over the domain-contract value lists, ultracode toggle (it IS
// `--effort ultracode`, so it disables the Effort picker), plan-mode toggle
// (default OFF — the session runs on an unattended desktop), plus a desktop
// picker when more than one is online. Last-used options persist via
// @AppStorage; stored values are validated against the contract on appear so
// a stale entry can never send a value the server rejects.

struct StartCodingSheet: View {
    let devices: [SteerDevice]
    let onStart: (SteerDevice, SteerStartOptions) -> Void

    @Environment(\.dismiss) private var dismiss

    /// Sentinel for the blank "CLI default" effort (omit --effort).
    private static let cliDefaultEffort = "cli-default"

    // Empty = "not chosen yet"; sanitizeStoredValues resolves it to the
    // contract's first value (first = default, per the contract convention),
    // so the default derives from the contract like on web/Android.
    @AppStorage("codingStart.model") private var model = ""
    @AppStorage("codingStart.effort") private var effort = Self.cliDefaultEffort
    @AppStorage("codingStart.ultracode") private var ultracode = false
    @AppStorage("codingStart.planMode") private var planMode = false
    @State private var deviceId: String?

    private var device: SteerDevice? {
        devices.first { $0.deviceId == deviceId } ?? devices.first
    }

    var body: some View {
        NavigationStack {
            Form {
                if devices.count > 1 {
                    Section {
                        Picker("Desktop", selection: deviceBinding) {
                            ForEach(devices) { device in
                                Text(device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel)
                                    .tag(device.deviceId)
                            }
                        }
                    }
                }

                Section {
                    Picker("Model", selection: $model) {
                        ForEach(DomainContract.codingModelValues, id: \.self) { value in
                            Text(Self.modelLabel(value)).tag(value)
                        }
                    }
                    Picker("Effort", selection: $effort) {
                        Text("CLI default").tag(Self.cliDefaultEffort)
                        ForEach(DomainContract.codingEffortValues, id: \.self) { value in
                            Text(Self.effortLabel(value)).tag(value)
                        }
                    }
                    .disabled(ultracode)
                }

                Section {
                    Toggle("Ultracode", isOn: $ultracode)
                } footer: {
                    Text("Dynamic multi-agent workflows — overrides the effort level.")
                }

                Section {
                    Toggle("Plan mode", isOn: $planMode)
                } footer: {
                    Text("Starts with a plan that needs approval — from the web or at the desktop.")
                }
            }
            .navigationTitle("Start coding")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Start") { submit() }
                        .disabled(device == nil)
                }
            }
        }
        .presentationDetents([.medium, .large])
        .onAppear { sanitizeStoredValues() }
    }

    private var deviceBinding: Binding<String> {
        Binding(
            get: { device?.deviceId ?? "" },
            set: { deviceId = $0 }
        )
    }

    private func submit() {
        guard let device else { return }
        let options = SteerStartOptions(
            model: model,
            effort: effort == Self.cliDefaultEffort ? "" : effort,
            ultracode: ultracode,
            planMode: planMode
        )
        dismiss()
        onStart(device, options)
    }

    /// An unset model or a stored value from an older build outside today's
    /// contract lists falls back to the defaults instead of reaching the wire.
    private func sanitizeStoredValues() {
        if !DomainContract.codingModelValues.contains(model) {
            model = DomainContract.codingModelValues.first ?? ""
        }
        if effort != Self.cliDefaultEffort, !DomainContract.codingEffortValues.contains(effort) {
            effort = Self.cliDefaultEffort
        }
    }

    private static func modelLabel(_ value: String) -> String {
        value.prefix(1).uppercased() + value.dropFirst()
    }

    private static func effortLabel(_ value: String) -> String {
        value == "xhigh" ? "XHigh" : value.prefix(1).uppercased() + value.dropFirst()
    }
}
