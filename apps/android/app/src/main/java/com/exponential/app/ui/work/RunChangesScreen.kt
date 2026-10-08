package com.exponential.app.ui.work

import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.CHAT_RUN_NAME
import com.exponential.app.domain.Diff
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.chatRunSubject
import com.exponential.app.ui.components.LocalDetailHaze
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.issue.toDiffFile
import dev.chrisbanes.haze.rememberHazeState
import com.exponential.app.domain.Prompts

// EXP-1194: the review of a RUN's own issue-less pull request (Reviews →
// Agent runs) — the same Changes face an issue row opens, fed by
// `codingSessions.prFiles` through [ChangesViewModel] ([ChangesSource.Session]).
// The header names the run (its title, the Reviews row's), GitHub sits in the
// header's action slot like the issue Changes face, and the white Merge
// capsule merges through `codingSessions.mergePr` while the PR is open.

@Composable
fun RunChangesScreen(
    sessionId: String,
    onBack: () -> Unit,
) {
    val viewModel = hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(
        key = "run-changes:$sessionId",
    ) { factory -> factory.create(ChangesSource.Session(sessionId)) }
    val session by viewModel.session.collectAsStateWithLifecycle()
    val load by viewModel.load.collectAsStateWithLifecycle()
    val merging by viewModel.merging.collectAsStateWithLifecycle()
    val actionError by viewModel.actionError.collectAsStateWithLifecycle()

    val files: List<Diff.File> = remember(load) {
        when (val state = load) {
            is ChangesLoadState.Loaded -> state.files.map { it.toDiffFile() }
            else -> emptyList()
        }
    }
    // The Reviews row's own title: a chat run's subject, an action run's
    // snapshot, else "Chat".
    val title = session?.let { chatRunSubject(it) ?: it.actionName }
        ?: session?.branch
        ?: CHAT_RUN_NAME
    val prUrl = session?.prUrl?.takeIf { it.isNotBlank() }

    val merge = session?.takeIf { it.prState == "open" }?.let { run ->
        ChangesMergeControl(
            label = DomainContract.diffUiMergePr,
            // No recovery run here: Fix conflicts takes an issue-linked PR.
            loading = merging,
            error = actionError,
            confirmPrompt = Prompts.MergeRunPr.prompt(run.prNumber),
            onConfirm = { viewModel.mergePr() },
        )
    }

    // A refused merge toasts ONCE (the WorkScreen rule).
    val toaster = LocalToaster.current
    var toastedError by rememberSaveable { mutableStateOf<String?>(null) }
    LaunchedEffect(actionError) {
        val error = actionError
        if (error == null) {
            toastedError = null
        } else if (error != toastedError) {
            toastedError = error
            toaster.error(error)
        }
    }

    val hazeState = rememberHazeState()
    CompositionLocalProvider(LocalDetailHaze provides hazeState) {
        Scaffold(
            containerColor = Color.Transparent,
            topBar = {
                WorkTopBar(
                    title = title,
                    onBack = onBack,
                    verb = null,
                    verbEnabled = false,
                    onVerb = {},
                    action = prUrl?.let { url -> { GithubHeaderAction(url) } },
                    menu = null,
                )
            },
        ) { padding ->
            ChangesFace(
                padding = padding,
                files = files,
                prLoad = load,
                merge = merge,
            )
        }
    }
}
