package com.exponential.app.data.api

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// One changed file in an issue's PR. Mirrors the web PullFile (github-pr.ts).
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

@Serializable
data class PrFilesResult(
    val repo: String? = null,
    val prNumber: Int? = null,
    val files: List<PullFile> = emptyList(),
)

/**
 * EXP-1154: a pull request's title + body as GitHub holds them — the Guide's
 * PR-body fallback when no session filed one. Every field may be null.
 */
@Serializable
data class PrDescription(
    val repo: String? = null,
    val prNumber: Int? = null,
    val url: String? = null,
    val title: String? = null,
    val body: String? = null,
    val state: String? = null,
)
