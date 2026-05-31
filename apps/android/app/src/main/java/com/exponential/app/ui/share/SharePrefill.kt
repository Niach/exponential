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
 * Map a shared payload onto a [SharePrefill]:
 *  - title = subject, else the first line of shared text, else "Shared image".
 *  - description = the shared text (full when subject supplied the title, else
 *    the remainder after the first line), with each shared image appended as a
 *    `![](draft://…)` block so the existing image-upload pipeline picks it up.
 */
fun buildSharePrefill(share: DeepLinkBus.Target.ShareContent): SharePrefill {
    val text = share.text?.trim().orEmpty()
    val subject = share.subject?.trim().orEmpty()

    val title = when {
        subject.isNotBlank() -> subject
        text.isNotBlank() -> text.lineSequence().firstOrNull()?.trim()?.take(120).orEmpty()
        else -> "Shared image"
    }

    val baseDescription = when {
        subject.isNotBlank() -> text
        text.contains('\n') -> text.substringAfter('\n').trim()
        else -> ""
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
