import ExpCore
import ExpUI
import SwiftUI

/// Full-screen blocking gate (EXP-104) shown when the server has rejected this
/// client version (HTTP 426). Mirrors the glass styling of LoginView /
/// InstanceView. There is no dismiss — the app stays here until the user
/// updates and relaunches with a supported build.
struct UpdateRequiredView: View {
    var body: some View {
        ZStack {
            AppBackground()

            VStack(alignment: .leading, spacing: 0) {
                Image(systemName: "arrow.up.circle")
                    .font(.system(size: 44, weight: .semibold))
                    .foregroundStyle(.white)

                Spacer().frame(height: 20)

                Text("Update required")
                    .font(.system(size: 28, weight: .bold))
                    .foregroundStyle(.white)

                Spacer().frame(height: 8)

                Text(bodyText)
                    .font(.body)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                Spacer().frame(height: 24)

                actionSection
            }
            .padding(24)
            .glassCard()
            .padding(.horizontal, 32)
        }
    }

    private var bodyText: String {
        let base = "This version of Exponential is no longer supported. Update to the latest version to keep going."
        if let min = UpdateGate.shared.upgrade?.min {
            return "\(base) The minimum supported version is \(min)."
        }
        return base
    }

    @ViewBuilder
    private var actionSection: some View {
        if AppConstants.isStaging {
            // Staging ships via TestFlight, which has no direct in-app update
            // deep link — point the user there in words.
            Text("Update the app via TestFlight")
                .font(.body.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
                .background(Color.white.opacity(0.08))
                .clipShape(RoundedRectangle(cornerRadius: 10))
                .overlay(
                    RoundedRectangle(cornerRadius: 10)
                        .stroke(Color.white.opacity(0.15), lineWidth: 0.5)
                )
        } else {
            Link(destination: AppConstants.appStoreUrl) {
                Text("Update on the App Store")
                    .font(.body.weight(.medium))
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 14)
            }
            .background(Color.white.opacity(0.15))
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
            )
        }
    }
}
