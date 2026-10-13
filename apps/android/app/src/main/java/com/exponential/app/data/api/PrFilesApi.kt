package com.exponential.app.data.api

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// One changed file in a pull request (the Changes rows; `pullRequests.files`
// rows map onto it in ChangesViewModel).
@Serializable
data class PullFile(
    val filename: String,
    /** EXP-912: where a renamed/copied file came from — GitHub's raw key. */
    @SerialName("previous_filename") val previousFilename: String? = null,
    val status: String,
    val additions: Int = 0,
    val deletions: Int = 0,
    val patch: String? = null,
)
