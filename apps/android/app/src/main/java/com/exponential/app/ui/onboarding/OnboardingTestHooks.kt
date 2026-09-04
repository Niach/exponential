package com.exponential.app.ui.onboarding

import androidx.annotation.VisibleForTesting

/**
 * CAPTURE-ONLY seam for the screenshot suites (EXP-725).
 *
 * The wizard's later steps are only reachable by actually creating a team and
 * a board, which the styleguide lane must not do — it runs against the shared
 * seed and every mutation it makes leaks into every other lane's shots. So the
 * instrumentation test presets the step instead: set [startStep] before the
 * wizard mounts and it opens on `"invite"` or `"devices"` as soon as a team
 * resolves.
 *
 * Nothing in the product reads or writes this — no route, no setting, no deep
 * link. It exists so `StyleguideScreenshotsTest` can photograph
 * `sg_onboarding-invite` and `sg_onboarding-devices`, and it stays inert in
 * every real run (a null default that only the test process ever changes).
 */
@VisibleForTesting
object OnboardingTestHooks {
    /** `"invite"` or `"devices"`; anything else (and null) starts at step 0. */
    @Volatile
    var startStep: String? = null
}
