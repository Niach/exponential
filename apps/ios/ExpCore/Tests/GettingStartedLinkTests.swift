import Foundation
import XCTest
@testable import ExpCore

// EXP-698 r5: the two off-app destinations the getting-started checklist hands
// out. Both have a web twin the phone must match byte-for-byte — a download
// link that drifts sends people to a 404, and an install one-liner that drifts
// installs the daemon against the wrong instance (or, worse, silently against
// the cloud when the user is self-hosted).
final class GettingStartedLinkTests: XCTestCase {

    // apps/web/src/lib/desktop-download.ts — `DESKTOP_RELEASES_URL`. Phones
    // always get the releases PAGE, never a platform asset.
    func testTheDesktopStepPointsAtTheReleasesPage() {
        XCTAssertEqual(
            AppConstants.desktopReleasesUrl.absoluteString,
            "https://github.com/Niach/exponential/releases/latest"
        )
    }

    // apps/web/src/components/my-machines.tsx — `buildServerInstallSnippet`.
    // The script is served by the CLOUD marketing site for every instance, so
    // the target is always named explicitly via EXP_INSTANCE.
    func testTheServerStepCopiesTheInstanceScopedInstaller() {
        XCTAssertEqual(
            AppConstants.serverInstallSnippet(origin: "https://app.exponential.at"),
            "curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://app.exponential.at sh"
        )
    }

    // A self-hosted origin rides the same one-liner — there is no second
    // script and no cloud default.
    func testTheInstallerNamesASelfHostedOrigin() {
        let snippet = AppConstants.serverInstallSnippet(origin: "https://exp.example.com")
        XCTAssertTrue(snippet.contains("EXP_INSTANCE=https://exp.example.com"))
        XCTAssertFalse(snippet.contains("app.exponential.at"))
    }
}
