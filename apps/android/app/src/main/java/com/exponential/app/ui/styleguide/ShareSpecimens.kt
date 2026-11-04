package com.exponential.app.ui.styleguide

import android.graphics.Bitmap
import android.graphics.Canvas as AndroidCanvas
import android.graphics.Color as AndroidColor
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.ComposerAddFileChoice
import com.exponential.app.domain.ComposerAgentRow
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.agent.AgentComposer
import com.exponential.app.ui.agent.ComposerAgentRowView
import com.exponential.app.ui.agent.ComposerAttachMenu
import com.exponential.app.ui.agent.ComposerAttachPicker
import com.exponential.app.ui.agent.composerAgentRowState
import com.exponential.app.ui.share.ShareToScreen
import com.exponential.app.ui.share.ShareToState
import com.exponential.app.ui.theme.AppBackground
import java.io.File

// EXP-1635 (share-targets.json, composer-menu.json `addFile`): the share flow
// and the + menu's sub-choice, drawn by their real composables. Capture-only
// (StyleguideScreenshotsTest#captureOnlyScreens): the flow enters through a
// system intent, the menu is anchored inside the composer.

const val ShareToSpecimenTestId = "share-to-specimen"
const val ShareComposerSpecimenTestId = "share-composer-specimen"
const val ComposerAddFileSpecimenTestId = "composer-add-file-specimen"

private val specimenAgents = listOf(
    SessionTree.AgentRow("assistant", "t1", "Assistant", "bot", "#a1a1aa", busy = false, needsYou = false, assistant = true),
    SessionTree.AgentRow("release", "t1", "Release Manager", "rocket", "#60a5fa", busy = false, needsYou = false),
    SessionTree.AgentRow("triage", "t1", "Support Triage", "life-buoy", "#f472b6", busy = false, needsYou = false),
)

private fun specimenImage(context: android.content.Context, name: String, color: Int): File {
    val file = File(context.cacheDir, "$name.png")
    val bitmap = Bitmap.createBitmap(160, 160, Bitmap.Config.ARGB_8888)
    AndroidCanvas(bitmap).drawColor(color)
    file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    return file
}

@Composable
fun ShareToSpecimenScreen() {
    val context = LocalContext.current
    val images = remember {
        listOf(
            specimenImage(context, "share-a", AndroidColor.rgb(96, 165, 250)).toUri(),
            specimenImage(context, "share-b", AndroidColor.rgb(244, 114, 182)).toUri(),
        )
    }
    AppBackground {
        Box(Modifier.fillMaxSize().testTag(ShareToSpecimenTestId)) {
            ShareToScreen(
                state = ShareToState(
                    teams = listOf(
                        TeamEntity(id = "t1", name = "Exponential", slug = "exp", createdAt = "", updatedAt = ""),
                        TeamEntity(id = "t2", name = "Side project", slug = "side", createdAt = "", updatedAt = ""),
                    ),
                    teamId = "t1",
                    agents = specimenAgents,
                ),
                imageUris = images,
                onBack = {},
                onSelectTeam = {},
                onNewIssue = {},
                onPick = {},
            )
        }
    }
}

/** THE agent row over THE composer card, as a share leaves them; [attach] opens the + sub-choice. */
@Composable
private fun SpecimenComposer(testId: String, text: String, attachments: List<PendingAttachment>, attach: ComposerAttachPicker? = null) {
    var field by remember { mutableStateOf(TextFieldValue(text, TextRange(text.length))) }
    AppBackground {
        Box(Modifier.fillMaxSize().statusBarsPadding().imePadding().testTag(testId), contentAlignment = Alignment.BottomCenter) {
            Column(
                modifier = Modifier.fillMaxWidth().padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(ComposerAgentRow.ROW_TO_CARD.dp),
            ) {
                ComposerAgentRowView(
                    state = composerAgentRowState(specimenAgents, "me", "release"),
                    onPick = {},
                )
                AgentComposer(
                    value = field,
                    onValueChange = { field = it },
                    onValueRewrite = { field = it },
                    fieldModifier = Modifier,
                    placeholder = "Message Release Manager",
                    pendingImages = attachments,
                    imageError = null,
                    sending = false,
                    onOpenMenu = {},
                    attachMenu = { if (attach != null) ComposerAttachMenu(attach) },
                    onRemoveImage = {},
                    submitLabel = "Send",
                    canSubmit = true,
                    onSubmit = {},
                )
            }
        }
    }
}

@Composable
fun ShareComposerSpecimenScreen() {
    val context = LocalContext.current
    val image = remember { specimenImage(context, "share-a", AndroidColor.rgb(96, 165, 250)) }
    SpecimenComposer(
        testId = ShareComposerSpecimenTestId,
        text = "Crash report\n\nhttps://example.com/a\n\n[Image #1]",
        attachments = listOf(
            PendingAttachment(image.toUri(), image.readBytes(), image.name, "image/png", isImage = true),
            PendingAttachment("file:///report.pdf".toUri(), ByteArray(1), "report.pdf", "application/pdf", isImage = false),
        ),
    )
}

@Composable
fun ComposerAddFileSpecimenScreen() {
    val picker = remember { ComposerAttachPicker(ComposerAddFileChoice.visible(hasCamera = true)) {} }
    LaunchedEffect(Unit) { picker.open = true }
    SpecimenComposer(testId = ComposerAddFileSpecimenTestId, text = "", attachments = emptyList(), attach = picker)
}
