package com.exponential.app.domain

/**
 * VAPP-241 (fixtures/app-share.json): an app's PUBLIC LINKS. Mirrors the TS
 * reference `packages/domain-contract/src/app-share.ts`; [AppShareTest]
 * replays the cases. The server computes each row's state; phones only draw it.
 */
object AppShare {
    const val PER_IP_PER_HOUR = 10
    const val PER_LINK_PER_HOUR = 200

    /** The `via` prefix of a guest's submission (`share:<name>`). */
    const val VIA_PREFIX = "share:"

    /** A link's states (`linkState`). */
    object State {
        const val OFF = "off"
        const val ON = "on"
        const val EXPIRED = "expired"
        const val ORPHANED = "orphaned"
    }

    /** `appAccess.setShareLink`'s `action`. */
    object Action {
        const val ENABLE = "enable"
        const val ROTATE = "rotate"
        const val REVOKE = "revoke"
        const val EXPIRE = "expire"
    }

    /** VAPP-247: one item of the row's expiry menu; [days] null = never. */
    data class ExpiryChoice(val days: Int?, val label: String)

    /** Fixture `expiryChoices`, in menu order. */
    val EXPIRY_CHOICES = listOf(
        ExpiryChoice(7, "Expires in 7 days"),
        ExpiryChoice(30, "Expires in 30 days"),
        ExpiryChoice(90, "Expires in 90 days"),
        ExpiryChoice(null, "Never expires"),
    )

    object Copy {
        const val TITLE = "Public links"
        const val SUBMIT = "Form"
        const val VIEW = "View"
        const val OFF = "Off"
        const val ON = "On"
        const val EXPIRED = "Expired"
        const val ORPHANED = "No longer in the app"
        const val EXPIRES_ON = "until {date}"
        const val COPY_LINK = "Copy link"
        const val NEW_LINK = "New link"
        const val GUEST = "Guest"
    }

    object TestIds {
        const val SHARE_LINKS = "app-settings-share-links"
        const val SHARE_ROW = "app-settings-share-row"
    }

    /** The two columns of a link row [linkState] reads. */
    data class Link(val revokedAt: String?, val expiresAt: String?)
}

/** A link's state at [now] (`stateCases`); [declared] = the published package
 *  still declares the share. */
fun linkState(link: AppShare.Link?, declared: Boolean, now: String): String {
    if (link == null || !link.revokedAt.isNullOrEmpty()) return AppShare.State.OFF
    if (!declared) return AppShare.State.ORPHANED
    val expiresAt = link.expiresAt?.takeIf { it.isNotEmpty() }?.let(WireTimestamps::parseEpochMs)
    val nowMs = WireTimestamps.parseEpochMs(now)
    if (expiresAt != null && nowMs != null && expiresAt <= nowMs) return AppShare.State.EXPIRED
    return AppShare.State.ON
}

/** May one more submission land (`capCases`)? */
fun underShareCaps(ipCount: Int, linkCount: Int): Boolean =
    ipCount < AppShare.PER_IP_PER_HOUR && linkCount < AppShare.PER_LINK_PER_HOUR

/** The By/author label (`bylineCases`): a shared record reads Guest. */
fun shareByline(shareName: String?, byline: String): String =
    if (shareName.isNullOrEmpty()) byline else AppShare.Copy.GUEST

/** A history event's `via` is a guest's submission (`historyCases`). */
fun isShareVia(via: String?): Boolean = via?.startsWith(AppShare.VIA_PREFIX) == true

/** A Settings row's Switch (`switchCases`): an orphaned link still reads on,
 *  so its owner can revoke it. */
fun shareSwitchOn(state: String): Boolean = state == AppShare.State.ON || state == AppShare.State.ORPHANED

/** A Settings row's subtitle (`rowCases`). */
fun shareRowSubtitle(mode: String, state: String, expiresAt: String?): String {
    val parts = mutableListOf(
        if (mode == "view") AppShare.Copy.VIEW else AppShare.Copy.SUBMIT,
        when (state) {
            AppShare.State.ON -> AppShare.Copy.ON
            AppShare.State.EXPIRED -> AppShare.Copy.EXPIRED
            AppShare.State.ORPHANED -> AppShare.Copy.ORPHANED
            else -> AppShare.Copy.OFF
        },
    )
    if (state == AppShare.State.ON && !expiresAt.isNullOrEmpty()) {
        parts += AppShare.Copy.EXPIRES_ON.replace("{date}", expiresAt)
    }
    return parts.joinToString(" · ")
}
