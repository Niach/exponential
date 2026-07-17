import ExpUI
import ExpCore
import SwiftUI

// The unified Start-coding sheet (EXP-156) — the iOS twin of the desktop IDE's
// single Start-coding dialog. A searchable multi-issue picker (checked rows
// pinned first) over Model / Effort pickers, an ultracode toggle (it IS
// `--effort ultracode`, so it disables the Effort picker), a plan-mode toggle,
// plus a desktop picker when more than one is online. 1 checked issue launches
// a plain single-issue session; 2+ launch ONE batch session on a shared
// `exp/batch-<id8>` branch ending in ONE combined PR.
//
// Last-used Model/Effort persist on every submit; ultracode/plan-mode persist
// ONLY on single-issue submits (batch seeding — flipping ultracode on / plan
// off when a 2nd issue is checked — must not pollute the stored single-issue
// defaults). Stored values are validated against the contract on appear so a
// stale entry can never send a value the server rejects.

struct StartCodingSheet: View {
    /// One eligible issue offered in the picker. `repositoryId` drives the
    /// single-repository-per-run validation (all checked issues must share one).
    struct IssueOption: Identifiable, Sendable {
        let id: String
        let identifier: String?
        let title: String
        let repositoryId: String?
        // Wire status/priority strings, so the picker rows can render the same
        // status/priority glyphs as the issue list (EXP-173). No defaults: a
        // producer that forgets them must fail to compile, not silently render
        // every row as Backlog/no-priority via IssueStatus/IssuePriority.from.
        let status: String?
        let priority: String?
    }

    let devices: [SteerDevice]
    /// Eligible candidates, pre-checked ids first (the current issue on the
    /// detail card, the whole pool on the Agents tab).
    let issues: [IssueOption]
    let preselectedIds: Set<String>
    let preferredDeviceId: String?
    let onStart: (SteerDevice, [String], SteerStartOptions) -> Void

    @Environment(\.dismiss) private var dismiss

    /// Sentinel for the blank "CLI default" effort (omit --effort).
    private static let cliDefaultEffort = "cli-default"
    /// A batch run is deliberately loose but not unbounded — one Claude session
    /// on one branch; past this the prompt is unwieldy and token-expensive.
    private static let maxBatchIssues = 30
    /// Above this we warn about token cost (still allowed up to the hard cap).
    private static let costWarnThreshold = 6

    private enum Keys {
        static let model = "codingStart.model"
        static let effort = "codingStart.effort"
        static let ultracode = "codingStart.ultracode"
        static let planMode = "codingStart.planMode"
    }

    @State private var checked: Set<String>
    @State private var searchText = ""
    @State private var deviceId: String?

    // Seeded from UserDefaults in onAppear (was @AppStorage). Placeholder
    // defaults render for one frame before seed() resolves them.
    @State private var model = ""
    @State private var effort = Self.cliDefaultEffort
    @State private var ultracode = false
    @State private var planMode = false
    // The persisted single-issue values, remembered so a batch→single toggle
    // can restore them (the auto batch defaults never touch UserDefaults).
    @State private var storedUltracode = false
    @State private var storedPlanMode = false
    @State private var seeded = false
    // Set by any manual Model/Effort/ultracode/plan interaction — once true the
    // auto batch-mode defaults stop overriding the user's explicit choices.
    @State private var touchedToggles = false

    init(
        devices: [SteerDevice],
        issues: [IssueOption],
        preselectedIds: Set<String>,
        preferredDeviceId: String? = nil,
        onStart: @escaping (SteerDevice, [String], SteerStartOptions) -> Void
    ) {
        self.devices = devices
        self.issues = issues
        self.preselectedIds = preselectedIds
        self.preferredDeviceId = preferredDeviceId
        self.onStart = onStart
        _checked = State(initialValue: preselectedIds)
    }

    private var device: SteerDevice? {
        if let deviceId, let match = devices.first(where: { $0.deviceId == deviceId }) {
            return match
        }
        if let preferredDeviceId, let match = devices.first(where: { $0.deviceId == preferredDeviceId }) {
            return match
        }
        return devices.first
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    searchField
                    if checkedRows.isEmpty, uncheckedRows.isEmpty {
                        Text(issues.isEmpty ? "No eligible issues to code." : "No matching issues.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } else if checkedRows.count + uncheckedRows.count <= 6 {
                        ForEach(checkedRows) { issueRow($0) }
                        ForEach(uncheckedRows) { issueRow($0) }
                    } else {
                        // Many candidates: scroll them inside a bounded box (one
                        // section row) so the Model / Effort / toggle sections
                        // stay near the top instead of being pushed off-screen.
                        ScrollView {
                            LazyVStack(spacing: 0) {
                                ForEach(checkedRows) { issueRow($0).padding(.vertical, 6) }
                                ForEach(uncheckedRows) { issueRow($0).padding(.vertical, 6) }
                            }
                        }
                        .frame(maxHeight: 280)
                    }
                } header: {
                    Text("Issues")
                } footer: {
                    issuesFooter
                }

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
                    Picker("Model", selection: modelBinding) {
                        ForEach(DomainContract.codingModelValues, id: \.self) { value in
                            Text(Self.modelLabel(value)).tag(value)
                        }
                    }
                    Picker("Effort", selection: effortBinding) {
                        Text("CLI default").tag(Self.cliDefaultEffort)
                        ForEach(DomainContract.codingEffortValues, id: \.self) { value in
                            Text(Self.effortLabel(value)).tag(value)
                        }
                    }
                    .disabled(ultracode)
                }

                Section {
                    Toggle("Ultracode", isOn: ultracodeBinding)
                } footer: {
                    Text("Dynamic multi-agent workflows — overrides the effort level.")
                }

                Section {
                    Toggle("Plan mode", isOn: planModeBinding)
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
                    Button(startTitle) { submit() }
                        .disabled(!canStart)
                }
            }
        }
        .presentationDetents([.medium, .large])
        .onAppear { seed() }
        // Crossing into/out of batch flips the mode defaults — unless the user
        // has already touched the option controls. Tracks the IN-POOL count so a
        // stray preselected id can't be mistaken for a real second issue.
        .onChange(of: effectiveChecked.count) { oldCount, newCount in
            guard !touchedToggles else { return }
            if oldCount < 2, newCount >= 2 {
                ultracode = true
                planMode = false
            } else if oldCount >= 2, newCount < 2 {
                ultracode = storedUltracode
                planMode = storedPlanMode
            }
        }
    }

    // MARK: - Issue picker

    private var searchField: some View {
        // Inline search field. NOT system .searchable — same rationale as
        // DuplicatePickerSheet (iOS 26 renders it as a bottom-edge glass bar).
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .font(.caption)
                .foregroundStyle(.secondary)
            TextField("Search issues", text: $searchText)
                .textFieldStyle(.plain)
                .submitLabel(.search)
            if !searchText.isEmpty {
                Button {
                    searchText = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func issueRow(_ option: IssueOption) -> some View {
        Button {
            toggle(option.id)
        } label: {
            HStack(spacing: 10) {
                Image(systemName: checked.contains(option.id) ? "checkmark.circle.fill" : "circle")
                    .font(.caption)
                    .foregroundStyle(checked.contains(option.id) ? Accent.indigo : .secondary)

                // Issue-list row anatomy (EXP-173): priority icon, mono
                // identifier, status icon, title.
                Image(systemName: IssuePriority.from(option.priority).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssuePriority.from(option.priority).color)
                    .frame(width: 16)

                Text(option.identifier ?? "")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .frame(minWidth: 60, alignment: .leading)

                Image(systemName: IssueStatus.from(option.status).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssueStatus.from(option.status).color)
                    .frame(width: 16)

                Text(option.title)
                    .font(.subheadline)
                    .lineLimit(1)

                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private var issuesFooter: some View {
        VStack(alignment: .leading, spacing: 4) {
            if multiRepo {
                Text("Pick issues from a single repository per run.")
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
            if overCap {
                Text("A batch run is capped at \(Self.maxBatchIssues) issues.")
                    .foregroundStyle(DesignTokens.Semantic.red)
            } else if effectiveChecked.count > Self.costWarnThreshold {
                Text("Large batches are token-expensive.")
            }
        }
    }

    private func matchesSearch(_ option: IssueOption) -> Bool {
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return true }
        return option.title.localizedCaseInsensitiveContains(trimmed)
            || (option.identifier ?? "").localizedCaseInsensitiveContains(trimmed)
    }

    /// Every id currently in the candidate pool.
    private var poolIds: Set<String> {
        Set(issues.map(\.id))
    }

    /// Checked ids that are actually in the pool. A preselected id whose issue
    /// isn't eligible (e.g. a repo-less current issue that startCodingCandidates
    /// deliberately left out) is a stray — it must never be shown, counted,
    /// validated, or submitted, so every derived value works off THIS masked set,
    /// not `checked`. This also self-heals if the pool arrives after seeding.
    private var effectiveChecked: Set<String> {
        checked.intersection(poolIds)
    }

    private var checkedRows: [IssueOption] {
        issues.filter { checked.contains($0.id) && matchesSearch($0) }
    }

    private var uncheckedRows: [IssueOption] {
        Array(issues.filter { !checked.contains($0.id) && matchesSearch($0) }.prefix(50))
    }

    /// Checked ids in the candidate pool's order (pre-checked / recency) — the
    /// launch payload. Filtering `issues` inherently drops strays.
    private var orderedCheckedIds: [String] {
        issues.filter { effectiveChecked.contains($0.id) }.map(\.id)
    }

    private var checkedRepoIds: Set<String> {
        Set(issues.filter { effectiveChecked.contains($0.id) }.compactMap(\.repositoryId))
    }

    private var multiRepo: Bool { checkedRepoIds.count > 1 }
    private var overCap: Bool { effectiveChecked.count > Self.maxBatchIssues }

    private var canStart: Bool {
        device != nil && !effectiveChecked.isEmpty && !multiRepo && !overCap
    }

    private var startTitle: String {
        effectiveChecked.count > 1 ? "Start coding (\(effectiveChecked.count) issues)" : "Start coding"
    }

    private func toggle(_ id: String) {
        if checked.contains(id) {
            checked.remove(id)
        } else {
            checked.insert(id)
        }
    }

    // MARK: - Bindings (touch tracking)

    private var deviceBinding: Binding<String> {
        Binding(
            get: { device?.deviceId ?? "" },
            set: { deviceId = $0 }
        )
    }

    // Manual interaction goes through these setters (programmatic seed / auto
    // batch defaults write the @State directly, so they don't mark touched).
    private var modelBinding: Binding<String> {
        Binding(get: { model }, set: { model = $0; touchedToggles = true })
    }

    private var effortBinding: Binding<String> {
        Binding(get: { effort }, set: { effort = $0; touchedToggles = true })
    }

    private var ultracodeBinding: Binding<Bool> {
        Binding(get: { ultracode }, set: { ultracode = $0; touchedToggles = true })
    }

    private var planModeBinding: Binding<Bool> {
        Binding(get: { planMode }, set: { planMode = $0; touchedToggles = true })
    }

    // MARK: - Seed / submit

    private func seed() {
        guard !seeded else { return }
        seeded = true
        let defaults = UserDefaults.standard
        model = defaults.string(forKey: Keys.model) ?? ""
        effort = defaults.string(forKey: Keys.effort) ?? Self.cliDefaultEffort
        storedUltracode = defaults.bool(forKey: Keys.ultracode)
        storedPlanMode = defaults.bool(forKey: Keys.planMode)
        ultracode = storedUltracode
        planMode = storedPlanMode
        sanitizeStoredValues()
        // Opening already in batch (2+ in-pool preselected) applies the batch
        // defaults.
        if effectiveChecked.count >= 2 {
            ultracode = true
            planMode = false
        }
    }

    private func submit() {
        guard let device, !orderedCheckedIds.isEmpty else { return }
        let ids = orderedCheckedIds
        let options = SteerStartOptions(
            model: model,
            effort: effort == Self.cliDefaultEffort ? "" : effort,
            ultracode: ultracode,
            planMode: planMode
        )
        let defaults = UserDefaults.standard
        defaults.set(model, forKey: Keys.model)
        defaults.set(effort, forKey: Keys.effort)
        // Only single-issue submits persist ultracode/plan — batch seeding must
        // not overwrite the stored single-issue defaults.
        if ids.count == 1 {
            defaults.set(ultracode, forKey: Keys.ultracode)
            defaults.set(planMode, forKey: Keys.planMode)
        }
        dismiss()
        onStart(device, ids, options)
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
