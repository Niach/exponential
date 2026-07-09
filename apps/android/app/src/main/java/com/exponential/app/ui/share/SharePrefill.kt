package com.exponential.app.ui.share

import android.net.Uri
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.ui.markdown.draftUrl

/**
 * Initial values for the create-issue sheet, derived from shared content.
 * [pendingImages] maps a `draft://` placeholder (already embedded in
 * [description] as `![](placeholder)`) to its cached image Uri — exactly the
 * shape `IssueListViewModel.createIssue` consumes.
 */
data class SharePrefill(
    val title: String,
    val description: String,
    val pendingImages: Map<String, Uri>,
)

/**
 * Map a shared payload onto a [SharePrefill] (iOS ShareItemExtractor parity):
 *  - title = the first line of shared text/URL (max 120 chars), else the
 *    subject for text/link shares, else EMPTY — an image/file-only share never
 *    prefills a filename as the title (EXTRA_SUBJECT is dropped when the payload
 *    is images only).
 *  - description = the shared text (full when the subject supplied the title,
 *    else the remainder after the title line), with each shared image appended
 *    as a `![](draft://…)` block so the existing image-upload pipeline picks it
 *    up.
 */
fun buildSharePrefill(share: DeepLinkBus.Target.ShareContent): SharePrefill {
    val text = share.text?.trim().orEmpty()
    val subject = share.subject?.trim().orEmpty()

    // The subject (a link's page title) is only a sensible title for shares that
    // carry text/URL content — for image/file-only shares it's the filename, so
    // drop it and leave the title empty for the user to type.
    val useSubject = subject.isNotBlank() && share.imageUris.isEmpty()
    val title = when {
        text.isNotBlank() -> text.lineSequence().firstOrNull()?.trim()?.take(120).orEmpty()
        useSubject -> subject.take(120)
        else -> ""
    }

    val baseDescription = when {
        text.isBlank() || text == title -> ""
        // The title took the first line — keep the rest as the description.
        text.startsWith(title) -> text.removePrefix(title).trim()
        else -> text
    }

    val pendingImages = LinkedHashMap<String, Uri>()
    val sb = StringBuilder(baseDescription)
    for (uri in share.imageUris) {
        val placeholder = draftUrl()
        pendingImages[placeholder] = uri
        if (sb.isNotEmpty()) sb.append("\n\n")
        sb.append("![](").append(placeholder).append(")")
    }

    return SharePrefill(title = title, description = sb.toString(), pendingImages = pendingImages)
}
