package com.exponential.app.ui.components

/**
 * EXP-1169: the device-setup server card's words, the same on all four
 * clients (web `lib/device-setup-copy.ts`; `device-setup-copy.test.ts` reads
 * this file and matches every string as a literal, so keep them plain
 * double-quoted literals). The card's title and description are the
 * getting-started server entry ([com.exponential.app.ui.gettingstarted.GettingStartedCopy]).
 */
object DeviceSetupCopy {
    const val COPY_COMMAND = "Copy install command"
    const val CODE_LABEL = "If the CLI shows a code, enter it here"
    const val CODE_PLACEHOLDER = "XXXX-XXXX"
    const val APPROVE = "Approve"
    const val APPROVED = "Code approved. The CLI signs in within a few seconds."
    const val CODE_USED = "That code has already been used. Run the login command again."
    const val CODE_EXPIRED = "That code has expired. Run the login command again to get a new one."
    const val CODE_INVALID = "That code isn't valid. Check for typos, or run the login command again."
    const val CODE_OTHER_ACCOUNT = "This code was requested from a different account."
    const val FAILED = "Something went wrong. Try again."
}
