import ExpCore
import ExpUI
import SwiftUI

/// EXP-1176 ×4: the signed-out / first-run heading — web's `BrandHeading`.
/// The ONE place the mark is drawn big: a 56pt logo centred over the title,
/// nothing beside it (the title names the product).
struct AuthBrandHeading: View {
    let title: String

    var body: some View {
        VStack(spacing: 12) {
            ExpLogoMark(size: 56)
                .frame(width: 56, height: 56)
                .accessibilityHidden(true)
            Text(title)
                .font(.system(size: 24, weight: .semibold))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
    }
}

/// The muted "Privacy · Terms" pair under the sign-in list ×4 (web's
/// `AuthFormShell` footer, same URLs).
struct AuthLegalFooter: View {
    @Environment(\.openURL) private var openURL

    var body: some View {
        HStack(spacing: 4) {
            link("Privacy", url: AppConstants.privacyUrl, identifier: "auth-privacy-link")
            Text("·")
            link("Terms", url: AppConstants.termsUrl, identifier: "auth-terms-link")
        }
        .font(.caption)
        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        .frame(maxWidth: .infinity)
    }

    private func link(_ label: String, url: URL, identifier: String) -> some View {
        Button(label) { openURL(url) }
            .buttonStyle(.plain)
            .accessibilityIdentifier(identifier)
    }
}
