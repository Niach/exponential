import ExpCore
import ExpUI
import SwiftUI

/// Settings → About (EXP-262): the app's first version surface, plus the
/// third-party licence acknowledgements every distributed build must carry.
/// The licences themselves are one push further (`AppRoute.thirdPartyLicenses`)
/// so the blob never weighs down this screen.
struct AboutView: View {
    private static let sourceUrl = URL(string: "https://github.com/Niach/exponential")!
    private static let licenseUrl =
        URL(string: "https://github.com/Niach/exponential/blob/master/LICENSE")!

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    appSection
                    linksSection
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 24)
            }
        }
        .navigationTitle("About")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
    }

    private var appSection: some View {
        HStack(spacing: 12) {
            AppIcon(AppIcons.settingsAbout, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 22)
            VStack(alignment: .leading, spacing: 2) {
                Text("Exponential")
                    .font(.body)
                    .foregroundStyle(.white)
                Text("Version \(AppConstants.appVersion)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            Spacer()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .glassRow()
    }

    private var linksSection: some View {
        VStack(spacing: 6) {
            NavigationLink(value: AppRoute.thirdPartyLicenses) {
                row(icon: AppIcons.settingsLicenses, title: "Third-party licenses") {
                    AppIcon(AppIcons.uiChevronRight, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                }
            }
            .buttonStyle(.plain)

            Link(destination: Self.sourceUrl) {
                row(icon: AppIcons.uiGithub, title: "Source code") {
                    AppIcon(AppIcons.uiExternalLink, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                }
            }
            .buttonStyle(.plain)

            Link(destination: Self.licenseUrl) {
                row(icon: AppIcons.uiInfo, title: "License (Apache-2.0)") {
                    AppIcon(AppIcons.uiExternalLink, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                }
            }
            .buttonStyle(.plain)
        }
    }

    private func row<Trailing: View>(
        icon: String,
        title: String,
        @ViewBuilder trailing: () -> Trailing
    ) -> some View {
        HStack(spacing: 12) {
            AppIcon(icon, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 22)
            Text(title)
                .font(.body)
                .foregroundStyle(.white)
            Spacer()
            trailing()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .glassRow()
    }
}
