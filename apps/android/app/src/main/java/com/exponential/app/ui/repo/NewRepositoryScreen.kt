package com.exponential.app.ui.repo

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import at.exponential.ui.primitives.ChoiceTile
import at.exponential.ui.primitives.ChoiceTiles
import at.exponential.ui.primitives.ChoiceTilesStyle
import at.exponential.ui.primitives.LocalPrimitiveTokens
import com.exponential.app.data.api.CiApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.domain.RepoPage
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.PinnedScaffold
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.TextFieldRow
import com.exponential.app.ui.onboarding.GithubRepoPickerList
import com.exponential.app.ui.theme.glassGroup
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

const val NEW_REPOSITORY_TEST_ID = "new-repository-page"

/** The form's fields (repo-page.json NEW › REPOSITORY, hosted). */
data class NewRepositoryForm(
    val name: String = "",
    val description: String = "",
    val defaultBranch: String = RepoPage.ADD_BRANCHES.first(),
    /** On by default: our CI (`ci.setRepoConfig {enabled: true}`). */
    val runCi: Boolean = true,
    val mirror: Boolean = false,
    val mirrorUrl: String = "",
) {
    val valid: Boolean get() = RepoPage.isValidName(name) && (!mirror || mirrorUrl.trim().startsWith("https://"))
}

/**
 * #EXP-2171 New › Repository (owners): `repositories.create`, then (Run CI on)
 * `ci.setRepoConfig {enabled: true}`; the created repository opens its page.
 * A failure is a toast.
 */
@HiltViewModel
class NewRepositoryViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    private val api: RepositoriesApi,
    private val ciApi: CiApi,
) : ViewModel() {
    val teamId: String = savedStateHandle.get<String>("teamId").orEmpty()
    val accountId: String? get() = auth.activeAccountId.value

    private val _busy = MutableStateFlow(false)
    val busy: StateFlow<Boolean> = _busy

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    fun create(form: NewRepositoryForm, onCreated: (repositoryId: String) -> Unit) {
        val accountId = auth.activeAccountId.value ?: return
        if (_busy.value || !form.valid) return
        _busy.value = true
        viewModelScope.launch {
            try {
                val id = api.createHosted(
                    accountId,
                    teamId,
                    form.name,
                    form.description.trim().ifEmpty { null },
                    form.defaultBranch,
                    if (form.mirror) form.mirrorUrl.trim() else null,
                )
                // The repository exists either way: a refused CI switch is
                // its own toast, the page still opens.
                if (form.runCi) {
                    runCatching { ciApi.setRepoConfig(accountId, id, enabled = true) }.onFailure {
                        if (it is CancellationException) throw it
                        _failure.value = FailedAction("turn on", "CI", trpcErrorMessage(it, ""))
                    }
                }
                onCreated(id)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("create", "the repository", trpcErrorMessage(t, "")) { create(form, onCreated) }
            } finally {
                _busy.value = false
            }
        }
    }

    /** Link from GitHub: THE connect/picker flow's add (`repositories.add`). */
    suspend fun link(fullName: String, defaultBranch: String, isPrivate: Boolean) {
        val accountId = auth.activeAccountId.value ?: return
        api.add(accountId, teamId, fullName, defaultBranch, isPrivate)
    }
}

/**
 * #EXP-2171 (repo-page.json NEW › REPOSITORY): ONE pushed page titled
 * `copy.new.title`: `ChoiceTiles` Host on Exponential (first, preselected) |
 * Link from GitHub. Hosted = Name, Description, Default branch, Run CI,
 * Mirror to GitHub revealing its URL, then Create repository. GitHub = the
 * connect list (the repository picker; Connect GitHub without a connection).
 */
@Composable
fun NewRepositoryScreen(
    onBack: () -> Unit,
    /** A created repository opens its page (replacing this one). */
    onCreated: (teamId: String, repositoryId: String) -> Unit,
    viewModel: NewRepositoryViewModel = hiltViewModel(),
) {
    val busy by viewModel.busy.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    NewRepositoryPage(
        busy = busy,
        onBack = onBack,
        onCreate = { form -> viewModel.create(form) { id -> onCreated(viewModel.teamId, id) } },
        github = { header ->
            val accountId = viewModel.accountId
            if (accountId != null) {
                GithubRepoPickerList(
                    accountId = accountId,
                    teamId = viewModel.teamId,
                    onAdd = { repo -> viewModel.link(repo.fullName, repo.defaultBranch, repo.isPrivate) },
                    onDone = onBack,
                    header = header,
                )
            }
        },
    )
}

/** The page itself, state hoisted ([github] = the connect list under the tiles). */
@Composable
fun NewRepositoryPage(
    busy: Boolean,
    onBack: () -> Unit,
    onCreate: (NewRepositoryForm) -> Unit,
    github: @Composable (header: @Composable () -> Unit) -> Unit,
) {
    var hosted by rememberSaveable { mutableStateOf(true) }
    var name by rememberSaveable { mutableStateOf("") }
    var description by rememberSaveable { mutableStateOf("") }
    var branch by rememberSaveable { mutableStateOf(RepoPage.ADD_BRANCHES.first()) }
    var runCi by rememberSaveable { mutableStateOf(true) }
    var mirror by rememberSaveable { mutableStateOf(false) }
    var mirrorUrl by rememberSaveable { mutableStateOf("") }
    val form = NewRepositoryForm(name, description, branch, runCi, mirror, mirrorUrl)
    val create = { onCreate(form) }

    val tiles = @Composable {
        ChoiceTiles(
            tiles = listOf(
                ChoiceTile(RepoPage.Copy.New.HOSTED, RepoPage.Copy.New.HOSTED_HINT, hasPicture = false, testTag = "new-repo-hosted"),
                ChoiceTile(RepoPage.Copy.New.GITHUB, RepoPage.Copy.New.GITHUB_HINT, hasPicture = false, testTag = "new-repo-github"),
            ),
            selected = setOf(if (hosted) 0 else 1),
            onToggle = { index -> hosted = index == 0 },
            style = ChoiceTilesStyle.default(LocalPrimitiveTokens.current).copy(columns = 2),
            modifier = Modifier.testTag("new-repo-provider"),
        )
    }

    PinnedScaffold(
        topBar = { PageHeader(title = RepoPage.Copy.New.TITLE, onBack = onBack, modifier = Modifier.statusBarsPadding()) },
    ) { padding ->
        if (!hosted) {
            Box(Modifier.padding(padding).fillMaxSize().testTag(NEW_REPOSITORY_TEST_ID)) {
                github { Box(Modifier.padding(horizontal = 16.dp)) { tiles() } }
            }
            return@PinnedScaffold
        }
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(16.dp)
                .testTag(NEW_REPOSITORY_TEST_ID),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            tiles()
            Column(Modifier.fillMaxWidth().glassGroup()) {
                TextFieldRow(
                    label = RepoPage.Copy.Add.NAME,
                    value = name,
                    onValueChange = { name = it.trim().take(100) },
                    placeholder = "app",
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next),
                    modifier = Modifier.testTag("new-repo-name"),
                )
                GroupDivider()
                TextFieldRow(
                    label = RepoPage.Copy.New.DESCRIPTION,
                    value = description,
                    onValueChange = { description = it.take(400) },
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next),
                    modifier = Modifier.testTag("new-repo-description"),
                )
                GroupDivider()
                PickerRow(
                    label = RepoPage.Copy.Add.DEFAULT_BRANCH,
                    value = branch,
                    options = RepoPage.ADD_BRANCHES,
                    selected = branch,
                    optionLabel = { it },
                    onSelect = { branch = it },
                )
                GroupDivider()
                SwitchRow(
                    title = RepoPage.Copy.New.RUN_CI,
                    checked = runCi,
                    onCheckedChange = { runCi = it },
                    modifier = Modifier.testTag("new-repo-run-ci"),
                )
                GroupDivider()
                SwitchRow(
                    title = RepoPage.Copy.Add.MIRROR,
                    checked = mirror,
                    onCheckedChange = { mirror = it },
                    modifier = Modifier.testTag("new-repo-mirror"),
                )
                if (mirror) {
                    GroupDivider()
                    TextFieldRow(
                        label = RepoPage.Copy.Add.MIRROR_URL,
                        value = mirrorUrl,
                        onValueChange = { mirrorUrl = it.take(400) },
                        placeholder = "https://github.com/owner/name",
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Done),
                        keyboardActions = KeyboardActions(onDone = { create() }),
                        modifier = Modifier.testTag("new-repo-mirror-url"),
                    )
                }
            }
            GlassSubmitButton(
                label = RepoPage.Copy.New.CREATE,
                onClick = { create() },
                enabled = form.valid && !busy,
                loading = busy,
                modifier = Modifier.testTag("new-repo-create"),
            )
        }
    }
}
