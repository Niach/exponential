import ExpCore
import ExpUI
import SwiftUI

/// VAPP-120: the styleguide's consent-card specimen (`sg_app-consent`): THE
/// card an app surface raises for a typed action nobody granted — the
/// operation's own words (`AppActions.actionConsent`), the arguments it would
/// send, Deny and Allow — drawn by the REAL `AppConsentCard` over a stand-in
/// surface. Capture only: XCUITest cannot reach the painted surface's rows,
/// so the live walk cannot raise it; the words and arguments are the
/// Working hours demo's "Post to the issue" on the docs, admin entry.
struct AppConsentSpecimens: View {
    @State private var gate = AppConsentGate()

    var body: some View {
        ZStack {
            AppBackground()
            VStack(alignment: .leading, spacing: 4) {
                Text("Working hours")
                    .font(.title2.weight(.semibold))
                Text("October 2026")
                    .foregroundStyle(.secondary)
                Spacer()
            }
            .padding(24)
        }
        .modifier(AppConsentCard(gate: gate))
        .task {
            let consent = AppActions.actionConsent(
                "exponential.comments.create",
                args: [(key: "issueId", value: .string("APP-15")), (key: "body", value: .string("Logged 8 h: docs, admin"))]
            )
            _ = await gate.ask(AppConsentWords(
                tool: consent?.tool ?? "exponential_comments",
                args: consent?.args,
                destructive: consent?.destructive ?? false,
                title: consent?.title
            ))
        }
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-cards-screen")
    }
}
