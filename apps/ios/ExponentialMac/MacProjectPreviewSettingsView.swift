import ExpCore
import ExpUI
import SwiftUI

/// Per-project "Run Targets & Preview" settings, opened from the workspace
/// settings projects section. Edits the DISPLAY MIRROR only: the read-only run
/// targets (id / name / platform, discovered from `.exponential/config.json` by
/// the desktop) plus the editable `feedbackProjectId` routing target. Build/run
/// commands are never shown here — they live only in the repo file. Owner-gated
/// server-side via the same `projects.updatePreviewConfig` mutation the web uses.
struct MacProjectPreviewSettingsView: View {
    @Environment(MacAppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss

    let accountId: String
    let project: ProjectEntity
    // Sibling projects in the same workspace — candidates for feedback routing.
    let workspaceProjects: [ProjectEntity]

    @State private var feedbackProjectId: String?
    @State private var saving = false
    @State private var error: String?
    @State private var loaded = false

    private var mirror: ProjectPreviewMirror? {
        MacPreviewConfig.parseMirror(project.previewConfig)
    }

    private var targets: [ProjectPreviewMirror.Target] {
        mirror?.targets ?? []
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Run Targets & Preview").font(.headline)
                Spacer()
                Button("Done") { dismiss() }
            }
            .padding()
            Divider()

            Form {
                targetsSection
                feedbackSection
                if let error {
                    Text(error).foregroundStyle(.red).font(.callout)
                }
            }
            .formStyle(.grouped)
        }
        .frame(width: 480, height: 480)
        .onAppear {
            guard !loaded else { return }
            loaded = true
            feedbackProjectId = mirror?.feedbackProjectId
        }
    }

    private var targetsSection: some View {
        Section("Run targets") {
            if targets.isEmpty {
                Text("No run targets yet. Targets are defined in `.exponential/config.json` in the linked repo and discovered automatically by the desktop app.")
                    .font(.caption).foregroundStyle(.secondary)
            } else {
                ForEach(targets) { target in
                    HStack(spacing: 10) {
                        if let platform = PreviewPlatform(wire: target.platform) {
                            Image(systemName: platform.sfSymbol).foregroundStyle(.secondary)
                            Text(target.name)
                            Spacer()
                            Text(platform.displayName)
                                .font(.caption).foregroundStyle(.secondary)
                                .padding(.horizontal, 6).padding(.vertical, 2)
                                .background(.quaternary, in: Capsule())
                        } else {
                            Text(target.name)
                            Spacer()
                            Text(target.platform).font(.caption).foregroundStyle(.tertiary)
                        }
                    }
                }
                Text("Defined in `.exponential/config.json` (read-only here).")
                    .font(.caption2).foregroundStyle(.tertiary)
            }
        }
    }

    private var feedbackSection: some View {
        Section("Feedback routing") {
            Picker("File feedback into", selection: $feedbackProjectId) {
                Text("This project").tag(String?.none)
                ForEach(workspaceProjects) { candidate in
                    Text(candidate.name).tag(String?.some(candidate.id))
                }
            }
            Text("Issues filed from the preview's annotate overlay land in this project.")
                .font(.caption).foregroundStyle(.secondary)
            Button(saving ? "Saving…" : "Save") { save() }
                .disabled(saving)
        }
    }

    private func save() {
        saving = true
        error = nil
        // Preserve the discovered targets; only the routing target is editable
        // here. The desktop repopulates `targets` after it parses the repo file.
        let mirrorTargets = targets.map {
            PreviewMirrorTarget(id: $0.id, name: $0.name, platform: $0.platform)
        }
        let input = UpdatePreviewConfigInput(
            projectId: project.id,
            previewConfig: PreviewMirrorInput(
                targets: mirrorTargets,
                feedbackProjectId: feedbackProjectId
            )
        )
        Task {
            do {
                try await deps.projectsApi.updatePreviewConfig(accountId: accountId, input)
                dismiss()
            } catch {
                self.error = error.localizedDescription
                saving = false
            }
        }
    }
}
