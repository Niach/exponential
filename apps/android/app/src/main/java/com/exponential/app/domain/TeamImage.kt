package com.exponential.app.domain

/**
 * #EXP-1645 (packages/domain-contract/fixtures/team-image.json): THE team mark
 * and the team image. Replays the TS reference `@exp/domain-contract/team-image`
 * (desktop `domain::team_image`, iOS ExpCore `TeamImage.swift`).
 */
object TeamImage {
    object Copy {
        const val ROW = "Team image"
        const val UPLOAD = "Upload"
        const val REMOVE = "Remove image"
    }

    object TestIds {
        const val ROW = "team-image"
        const val UPLOAD = "team-image-upload"
        const val REMOVE = "team-image-remove"
    }

    /** `mark`: the image's corner, the glyph without one. */
    const val RADIUS_FRACTION = 0.25f
    const val GLYPH = "ui-team"

    /** `row.markSize`: the mark in the Settings › General row (dp). */
    const val ROW_MARK_SIZE = 28

    /** `upload`: the one multipart part `file`, owners only. */
    object Upload {
        const val PATH = "/api/teams/{teamId}/icon"
        const val MAX_BYTES = 10_485_760L
        const val SIDE = 512
        val ACCEPT = listOf("image/png", "image/jpeg", "image/webp", "image/gif", "image/svg+xml")
    }

    sealed interface Mark {
        data class Image(val src: String) : Mark
        data object Glyph : Mark
    }

    /** The team's image when `icon_url` holds one, else the group glyph. */
    fun teamMark(iconUrl: String?): Mark {
        val src = iconUrl?.trim().orEmpty()
        return if (src.isEmpty()) Mark.Glyph else Mark.Image(src)
    }

    /** The upload route of a team. */
    fun uploadPath(teamId: String): String = Upload.PATH.replace("{teamId}", teamId)
}
