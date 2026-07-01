import AppKit
import ExpCore
import ExpUI
import SwiftUI

/// The dedicated resizable Preview pane (a trailing inspector in MacShell). Header
/// = target picker (named targets grouped by platform, remembers last-selected per
/// project) + Build/Run/Stop + Annotate toggle. Body = the embedded live app
/// (PreviewDockHost) with the annotate overlay layered on top while annotating.
struct MacPreviewPane: View {
    @Environment(MacAppDependencies.self) private var deps
    @Bindable var controller: MacPreviewController

    @State private var doctorReport: MacPreviewDoctor.Report?
    @State private var showTrustPrompt = false
    @State private var annotationModel: AnnotationModel?
    @State private var sendSheet: SendFeedbackPayload?
    @State private var capturing = false

    private var reporter: MacFeedbackReporter {
        MacFeedbackReporter(issuesApi: deps.issuesApi, imagesApi: deps.issueImagesApi)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            body(for: controller.phase)
        }
        .frame(minWidth: 300)
        .background(Color.black.opacity(0.2))
        .confirmationDialog(
            "Trust preview commands for \(controller.repo ?? "this repo")?",
            isPresented: $showTrustPrompt,
            titleVisibility: .visible
        ) {
            Button("Trust & Run") {
                controller.approveTrust()
                controller.run()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(trustMessage)
        }
        .sheet(item: $sendSheet) { payload in
            SendFeedbackSheet(
                accountId: payload.accountId,
                projectId: payload.projectId,
                targetName: payload.targetName,
                screenshot: payload.screenshot,
                thumbnail: payload.thumbnail,
                reporter: reporter,
                toasts: deps.toastCenter,
                onFiled: { controller.annotating = false; annotationModel = nil }
            )
            .preferredColorScheme(.dark)
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: 8) {
            targetPicker
            Spacer(minLength: 4)
            if controller.phase.isActive {
                ProgressView().controlSize(.small)
            }
            controls
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.bar)
    }

    @ViewBuilder
    private var targetPicker: some View {
        let grouped = Dictionary(grouping: controller.pickerTargets, by: \.platform)
        Menu {
            ForEach(PreviewPlatform.allCases, id: \.self) { platform in
                if let targets = grouped[platform], !targets.isEmpty {
                    Section(platform.displayName) {
                        ForEach(targets) { target in
                            Button {
                                controller.select(targetId: target.id)
                            } label: {
                                if controller.selectedTargetId == target.id {
                                    Label(target.name, systemImage: "checkmark")
                                } else {
                                    Text(target.name)
                                }
                            }
                        }
                    }
                }
            }
            if controller.pickerTargets.isEmpty {
                Text("No run targets — add .exponential/config.json").foregroundStyle(.secondary)
            }
        } label: {
            HStack(spacing: 6) {
                if let selected = controller.pickerTargets.first(where: { $0.id == controller.selectedTargetId }) {
                    Image(systemName: selected.platform.sfSymbol).foregroundStyle(.secondary)
                    Text(selected.name).lineLimit(1)
                } else {
                    Image(systemName: "play.rectangle").foregroundStyle(.secondary)
                    Text("Preview").foregroundStyle(.secondary)
                }
                Image(systemName: "chevron.up.chevron.down").font(.caption2).foregroundStyle(.tertiary)
            }
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    @ViewBuilder
    private var controls: some View {
        let running = isRunning
        if running {
            Button { controller.stop() } label: { Image(systemName: "stop.fill") }
                .help("Stop the preview")
            Button {
                controller.annotating ? cancelAnnotate() : beginAnnotate()
            } label: {
                Image(systemName: controller.annotating ? "pencil.slash" : "pencil.tip.crop.circle")
            }
            .help(controller.annotating ? "Stop annotating" : "Annotate & file feedback")
            .disabled(capturing)
        } else {
            Button { runOrPromptTrust() } label: {
                Label("Run", systemImage: "play.fill")
            }
            .disabled(!canRun)
            .help(canRun ? "Build & run the selected target" : "Select a runnable target")
        }
    }

    private var isRunning: Bool {
        if case .running = controller.phase { return true }
        return false
    }

    private var canRun: Bool {
        guard let id = controller.selectedTargetId else { return false }
        return controller.pickerTargets.first { $0.id == id }?.runnable == true
    }

    // MARK: - Body

    @ViewBuilder
    private func body(for phase: PreviewPhase) -> some View {
        ZStack {
            PreviewDockHost(controller: controller)
            switch phase {
            case .idle:
                idleState
            case .error(let message):
                errorState(message)
            case .needsMac:
                ContentUnavailableView(
                    "Needs a Mac", systemImage: "desktopcomputer",
                    description: Text("iOS Simulator preview runs on a Mac.")
                )
            case .running:
                if controller.annotating, let model = annotationModel {
                    MacPreviewAnnotateOverlay(
                        model: model,
                        onSend: { presentSendSheet(model: model) },
                        onCancel: { cancelAnnotate() }
                    )
                }
            default:
                // Active build/boot phases: a translucent status over the surface.
                statusOverlay(phase.label)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var idleState: some View {
        VStack(spacing: 12) {
            Image(systemName: "play.rectangle.on.rectangle").font(.largeTitle).foregroundStyle(.secondary)
            if controller.pickerTargets.isEmpty {
                Text("No run targets")
                    .font(.headline)
                Text(controller.repo == nil
                     ? "Link a GitHub repo in workspace settings to preview this project."
                     : "Add an `.exponential/config.json` to the repo to define run targets.")
                    .font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.center)
            } else if !canRun {
                Text("Target not runnable here")
                    .font(.headline)
                Text("The repo isn't cloned yet — the agent clones it on first assignment, then the commands become available.")
                    .font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.center)
            } else {
                Text("Press Run to build & embed the selected target.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let report = doctorReport, !report.allOk {
                doctorView(report)
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func errorState(_ message: String) -> some View {
        VStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill").font(.largeTitle).foregroundStyle(.orange)
            Text(message)
                .font(.callout).multilineTextAlignment(.center).foregroundStyle(.secondary)
                .textSelection(.enabled)
            Button("Retry") { controller.reloadTargets(); runOrPromptTrust() }
                .disabled(!canRun)
            if let report = doctorReport, !report.allOk {
                doctorView(report)
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.regularMaterial)
    }

    private func statusOverlay(_ text: String) -> some View {
        VStack(spacing: 10) {
            ProgressView()
            Text(text).font(.callout).foregroundStyle(.secondary)
        }
        .padding(24)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
    }

    private func doctorView(_ report: MacPreviewDoctor.Report) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(report.failures) { check in
                VStack(alignment: .leading, spacing: 2) {
                    Label(check.name, systemImage: "xmark.circle").foregroundStyle(.orange).font(.caption)
                    if let remediation = check.remediation {
                        Text(remediation).font(.caption2).foregroundStyle(.secondary)
                    }
                }
            }
        }
        .padding(10)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
    }

    // MARK: - Actions

    private func runOrPromptTrust() {
        // Run a doctor pass for the selected platform so failures show inline.
        if let platform = controller.selectedTarget?.platform ?? controller.pickerTargets.first(where: { $0.id == controller.selectedTargetId })?.platform {
            Task { doctorReport = await MacPreviewDoctor.run(for: platform) }
        }
        if controller.needsTrustPrompt {
            showTrustPrompt = true
        } else {
            controller.run()
        }
    }

    private var trustMessage: String {
        guard let target = controller.selectedTarget else {
            return "These commands run locally on your Mac."
        }
        let commands = target.commandSet.joined(separator: "\n")
        return "These commands from .exponential/config.json will run locally on your Mac:\n\n\(commands)"
    }

    private func beginAnnotate() {
        capturing = true
        Task {
            let frame = await controller.captureFrame()
            capturing = false
            guard let frame else {
                deps.toastCenter.show("Couldn't capture the preview frame.", style: .error)
                return
            }
            annotationModel = AnnotationModel(baseImage: frame)
            controller.annotating = true
        }
    }

    private func cancelAnnotate() {
        controller.annotating = false
        annotationModel = nil
    }

    private func presentSendSheet(model: AnnotationModel) {
        guard let accountId = controller.accountId,
              let projectId = controller.feedbackProjectId else {
            deps.toastCenter.show("No feedback project configured.", style: .error)
            return
        }
        let flattened = model.flatten()
        let screenshot = flattened.map {
            MacFeedbackReporter.Screenshot(
                data: $0.data,
                filename: $0.filename,
                contentType: contentType(for: $0.filename)
            )
        }
        let thumbnail = flattened.flatMap { MacAnnotationRenderer.decode($0.data) }
            .map { NSImage(cgImage: $0, size: .zero) }
        sendSheet = SendFeedbackPayload(
            accountId: accountId,
            projectId: projectId,
            targetName: controller.selectedTarget?.name ?? "Preview",
            screenshot: screenshot,
            thumbnail: thumbnail
        )
    }

    private func contentType(for filename: String) -> String {
        if filename.hasSuffix(".webp") { return "image/webp" }
        if filename.hasSuffix(".jpg") || filename.hasSuffix(".jpeg") { return "image/jpeg" }
        return "image/png"
    }
}

/// Identifiable payload so the send sheet can use `.sheet(item:)`.
struct SendFeedbackPayload: Identifiable {
    let id = UUID()
    let accountId: String
    let projectId: String
    let targetName: String
    let screenshot: MacFeedbackReporter.Screenshot?
    let thumbnail: NSImage?
}
