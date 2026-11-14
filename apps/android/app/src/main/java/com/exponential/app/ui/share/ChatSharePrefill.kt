package com.exponential.app.ui.share

import android.content.Context
import com.exponential.app.data.ChatShare
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.domain.ShareTargets
import com.exponential.app.domain.STEER_ATTACHMENT_REJECTED
import com.exponential.app.ui.agent.ComposerPick
import com.exponential.app.ui.agent.readComposerPick
import com.exponential.app.ui.markdown.MarkdownMediaUtils
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@dagger.hilt.EntryPoint
@dagger.hilt.InstallIn(dagger.hilt.components.SingletonComponent::class)
internal interface ShareSelectionEntryPoint {
    fun teamSelection(): TeamSelection
}

/** The app-singleton holding the waiting [ChatShare], for the composers. */
internal fun shareSelection(context: Context): TeamSelection =
    dagger.hilt.android.EntryPointAccessors
        .fromApplication(context.applicationContext, ShareSelectionEntryPoint::class.java)
        .teamSelection()

/** share-targets.json `shareChatText`: what the payload leaves in the composer. */
fun chatShareText(share: DeepLinkBus.Target.ShareContent): String = ShareTargets.shareChatText(
    text = share.text,
    subject = share.subject,
    url = null,
    hasImages = share.imageUris.isNotEmpty() || share.files.isNotEmpty(),
)

/**
 * Takes the waiting share once ([take]) and runs its [deliver] in [scope],
 * the SCREEN's scope, never the effect that noticed it: taking it nulls
 * `pendingChatShare`, which restarts an effect keyed on it, and a delivery
 * running there was cancelled at its first suspension (the first image's
 * read), so the text landed and every attachment was dropped.
 */
fun takeChatShare(scope: CoroutineScope, take: () -> Boolean, deliver: suspend () -> Unit): Boolean {
    if (!take()) return false
    scope.launch { deliver() }
    return true
}

/**
 * The prefill ([ChatShare]): the text, then each image and file as an ordinary
 * pick through the composer's own [onPick], so its caps + toasts apply.
 * Nothing is sent here. [onText] gets the text with the composer's own draft
 * kept ahead of it.
 */
suspend fun deliverChatShare(
    context: Context,
    share: DeepLinkBus.Target.ShareContent,
    currentDraft: String,
    onText: (String) -> Unit,
    onPick: (List<ComposerPick>) -> Unit,
) {
    val text = chatShareText(share)
    if (text.isNotEmpty()) {
        onText(listOf(currentDraft.trimEnd(), text).filter { it.isNotEmpty() }.joinToString("\n\n"))
    }
    // #EXP-1841: the share is ONE pick (images, then files), staged at once.
    val picks = mutableListOf<ComposerPick>()
    for (uri in share.imageUris) picks.add(readComposerPick(context, uri))
    for (file in share.files) {
        val bytes = withContext(Dispatchers.IO) { MarkdownMediaUtils.readBytes(context, file.uri) }
        picks.add(
            if (bytes == null) {
                ComposerPick.Refused("That file could not be read")
            } else {
                ComposerPick.Read(file.uri, bytes, file.filename, file.mime)
            },
        )
    }
    if (share.oversizeFiles) picks.add(ComposerPick.Refused(STEER_ATTACHMENT_REJECTED))
    if (picks.isNotEmpty()) onPick(picks)
}
