import ExpCore
import ExpUI
import SwiftUI
import UIKit

/// EXP-1635: capture only (`-uiTestingStyleguide share-to|share-composer|
/// composer-attach-menu`). The extension cannot be driven by a test, so the
/// shots draw its real pages (compiled into the app too) over sample rows.
struct ShareSpecimens: View {
    static let kinds: Set<String> = ["share-to", "share-composer", "share-composer-temporary", "composer-attach-menu"]

    let kind: String

    @State private var teamKey: String? = "a:t"

    private static let agents = [
        MirroredAgent(
            id: "assistant", teamId: "t", name: "Assistant", glyph: "bot", color: "#6366f1",
            sortOrder: 0, role: DomainContract.agentRoleAssistant, ownerId: "me", conversation: nil
        ),
        MirroredAgent(
            id: "release", teamId: "t", name: "Release Manager", glyph: "rocket", color: "#22c55e",
            sortOrder: 1, role: "agent", ownerId: "u2", conversation: nil
        ),
        MirroredAgent(
            id: "triage", teamId: "t", name: "Support Triage", glyph: "bug", color: "#f59e0b",
            sortOrder: 2, role: "agent", ownerId: "u2", conversation: nil
        ),
    ]

    private static let devices = [
        MirroredDevice(
            deviceId: "d1", rowIds: [], label: "Studio Mac", glyph: AppIcons.uiDevice,
            online: true, isDefault: true, isMine: true, sharedTeamIds: []
        ),
    ]

    /// A sample picture: the capture has no photo library.
    private static func sample(_ top: UIColor, _ bottom: UIColor) -> SharedImage {
        let size = CGSize(width: 160, height: 160)
        let image = UIGraphicsImageRenderer(size: size).image { context in
            let colors = [top.cgColor, bottom.cgColor] as CFArray
            guard let gradient = CGGradient(colorsSpace: nil, colors: colors, locations: nil) else { return }
            context.cgContext.drawLinearGradient(gradient, start: .zero, end: CGPoint(x: 160, y: 160), options: [])
        }
        return SharedImage(data: image.pngData() ?? Data(), filename: "shot.png", contentType: "image/png")
    }

    private static let images = [
        sample(.systemIndigo, .systemTeal),
        sample(.systemOrange, .systemPink),
    ]

    private static let payload = SharedPayload(
        title: "", descriptionText: "", images: images,
        text: "Login crash on the settings page", url: "https://example.com/crash"
    )

    var body: some View {
        Group {
            switch kind {
            case "composer-attach-menu": AttachMenuSpecimen()
            default: NavigationStack { page }
            }
        }
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat-cards-screen")
    }

    @ViewBuilder
    private var page: some View {
        switch kind {
        case "share-to":
            ShareToPage(
                teams: [ShareTeam(accountId: "a", teamId: "t", name: "Exponential")],
                teamKey: $teamKey,
                agents: Self.agents,
                images: Self.images
            )
            .navigationTitle(ShareTargets.Copy.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") {} }
            }
        default:
            ShareChatComposerPage(
                context: ShareComposerContext(
                    teamId: "t", agents: Self.agents, me: "me", devices: Self.devices, chat: nil
                ),
                pick: kind == "share-composer" ? "release" : ComposerAgentRow.temporaryPick,
                payload: Self.payload,
                onSend: { _, _, _ in },
                onComplete: {}
            )
        }
    }
}

/// THE Camera | Photos | File sub-choice, open (run with
/// `-uiTestingCameraRow`: a simulator has no camera).
private struct AttachMenuSpecimen: View {
    @State private var open = false

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            AppBackground()
            Color.clear
                .steerAttachChoiceMenu(
                    isPresented: $open,
                    anchor: CGRect(x: 24, y: 640, width: 36, height: 36),
                    onCamera: {},
                    onPhoto: {},
                    onFile: {}
                )
        }
        .task {
            try? await Task.sleep(for: .milliseconds(400))
            open = true
        }
    }
}
