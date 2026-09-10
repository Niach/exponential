package com.exponential.app.domain

import com.exponential.app.data.api.ActionInputDto

/**
 * EXP-825: the composer's typed action inputs on the wire — the Android twin of
 * iOS `ActionInputValues` and the web `lib/action-inputs.ts` payload builder.
 *
 * Only PICK types remain (`repo` / `board` / `pr` / `icon`, the contract's
 * `actionInputType` values): `text` and `textarea` were retired with the ONE
 * launcher — what the requester types reaches the run as the start's `prompt`
 * ("Additional instructions from the requester"), never as an input. A row
 * that still declares one (an older server, a not-yet-migrated action) reads
 * as UNSUPPORTED here: the composer blocks the run and says the action needs
 * a newer app version, exactly like a type this build has never heard of.
 */
object ActionInputValues {

    /** A def whose type this build cannot render — the run is blocked on it. */
    fun hasUnsupportedType(defs: List<ActionInputDto>): Boolean =
        defs.any { it.type !in DomainContract.actionInputTypeValues }

    /** Every required def has a non-blank value. */
    fun requiredFilled(defs: List<ActionInputDto>, values: Map<String, String>): Boolean =
        defs.filter { it.required }.all { !values[it.key].isNullOrBlank() }

    /**
     * The `inputs` payload: only FILLED values ride, keyed by the def key, in
     * def order, verbatim — a picked id or glyph name is never trimmed or
     * rewritten. A cleared optional pick stores "" and is dropped here, so an
     * empty string never reaches the server as a bogus id (EXP-756). Values
     * for keys the action does not declare are dropped too.
     */
    fun wireValues(defs: List<ActionInputDto>, values: Map<String, String>): Map<String, String> =
        buildMap {
            for (def in defs) {
                val value = values[def.key] ?: continue
                if (value.isBlank()) continue
                put(def.key, value)
            }
        }
}
