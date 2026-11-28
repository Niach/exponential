package at.exponential.ui.measure

import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import at.exponential.ui.R

/** The family the `exponential` theme names (`type.family.sans`) and the apps draw their text in. */
const val INTER_FAMILY = "Inter"

/**
 * #EXP-2215: Inter (rsms/inter 4.001, SIL OFL 1.1, `OFL-Inter.txt`), the
 * four weights the clients use: Regular, Medium, SemiBold, Bold
 * (`res/font/inter_*.ttf`, the very files the desktop and the conformance
 * set embed). A host that registers no `Inter` gets these, so genUI draws
 * Inter on Android like on the web and the desktop instead of Roboto; the
 * app takes the same copy for its own typography (`ExponentialUi.inter`).
 */
internal object InterFont {
    val bundled: FontFamily by lazy {
        FontFamily(
            Font(R.font.inter_regular, FontWeight.W400),
            Font(R.font.inter_medium, FontWeight.W500),
            Font(R.font.inter_semibold, FontWeight.W600),
            Font(R.font.inter_bold, FontWeight.W700),
        )
    }
}
