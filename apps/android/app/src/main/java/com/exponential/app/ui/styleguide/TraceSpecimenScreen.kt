package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.TaskQueueBackgroundInput
import com.exponential.app.domain.TaskQueueInput
import com.exponential.app.domain.TaskQueueTaskInput
import com.exponential.app.domain.TaskQueues
import com.exponential.app.domain.Trace as TraceModels
import com.exponential.app.domain.TraceStatus
import com.exponential.app.ui.components.CodeBlock
import com.exponential.app.ui.components.CodeBlockLanguage
import com.exponential.app.ui.components.TaskQueueView
import com.exponential.app.ui.components.Trace
import com.exponential.app.ui.components.TraceCollapsible
import com.exponential.app.ui.components.TraceLine
import com.exponential.app.ui.components.TraceSize
import com.exponential.app.ui.theme.AppBackground

/** Which EXP-1324 specimen a capture shows (one shot each). */
enum class TraceSpecimenKind(val testId: String) {
    Trace("trace-specimen-screen"),
    CodeBlock("code-block-specimen-screen"),
}

private val PROFILE_OUTPUT = listOf(
    "> Task :app:startupProfile",
    "Pixel 6a     p50 740 ms   p90 812 ms",
    "Galaxy A54   p50 790 ms   p90 840 ms",
    "BUILD SUCCESSFUL in 41s",
).joinToString("\n")

private val BENCHMARK_OUTPUT = listOf(
    "StartupBenchmark > coldStart FAILED",
    "    java.lang.AssertionError: p90 812 ms > 800 ms",
    "",
    "12 tests completed, 1 failed",
).joinToString("\n")

private const val TS_SOURCE = """export function coldStart(board: Board): Promise<void> {
  // Paint the cached snapshot first, subscribe after the first frame.
  const cached = snapshots.read(board.id)
  return render(cached ?? [], { deferSubscribe: true })
}"""

private val ANSI_OUTPUT = listOf(
    "$ ./gradlew :app:startupProfile",
    "> Task :app:startupProfile",
    "\u001b[33mwarning:\u001b[0m baseline profile is 3 days old",
    "Pixel 6a     p50 740 ms   p90 812 ms",
    "\u001b[32mBUILD SUCCESSFUL\u001b[0m in 41s",
).joinToString("\n")

/**
 * EXP-1324 (+ EXP-1321): the styleguide's Trace and CodeBlock on Android, a
 * capture-only screen (sg_trace, sg_code-block) drawn by the REAL primitives
 * over the contract models — the same specimen as the web islands
 * (apps/styleguide/src/entries/trace.tsx, code-block.tsx).
 */
@Composable
fun TraceSpecimenScreen(kind: TraceSpecimenKind) {
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(kind.testId)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .statusBarsPadding()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 72.dp),
                verticalArrangement = Arrangement.spacedBy(24.dp),
            ) {
                when (kind) {
                    TraceSpecimenKind.Trace -> TraceSpecimen()
                    TraceSpecimenKind.CodeBlock -> CodeBlockSpecimen()
                }
            }
        }
    }
}

@Composable
private fun TraceSpecimen() {
    var outputOpen by remember { mutableStateOf(true) }
    var testsOpen by remember { mutableStateOf(false) }
    var todoOpen by remember { mutableStateOf(true) }
    var doneOpen by remember { mutableStateOf(false) }
    val failedTests = TraceModels.failedTests(TraceModels.parseTestResults(BENCHMARK_OUTPUT))!!
    Trace(size = TraceSize.Sm) {
        TraceLine(TraceModels.tool("execute", "Bash", "./gradlew :app:startupProfile", settled = false, failed = false))
        TraceLine(TraceModels.reasoning(running = false, durationMs = 5_200))
        TraceCollapsible(
            open = outputOpen,
            onOpenChange = { outputOpen = it },
            trigger = { open, toggle ->
                TraceLine(
                    TraceModels.tool("execute", "Bash", "./gradlew :app:startupProfile", settled = true, failed = false),
                    open = open,
                    onClick = toggle,
                )
            },
        ) {
            CodeBlock(PROFILE_OUTPUT, language = CodeBlockLanguage.Ansi)
        }
        TraceLine(
            TraceModels.subagent("Explore", running = false, failed = false, durationMs = 72_000),
            badge = true,
            chevron = true,
            onClick = {},
        )
        TraceCollapsible(
            open = testsOpen,
            onOpenChange = { testsOpen = it },
            trigger = { open, toggle -> TraceLine(failedTests, open = open, onClick = toggle) },
        ) {
            CodeBlock(BENCHMARK_OUTPUT, language = CodeBlockLanguage.Ansi, destructive = true)
        }
        TraceLine(
            TraceModels.tool("read", "Read", "README.md", settled = true, failed = false)
                .copy(status = TraceStatus.Pending),
        )
    }
    val queue = remember {
        TaskQueues.of(
            TaskQueueInput(
                tasks = listOf(
                    TaskQueueTaskInput("Profile the cold start", "completed"),
                    TaskQueueTaskInput("Defer the snapshot cache warm-up", "completed"),
                    TaskQueueTaskInput("Measuring on the Galaxy A54", "in_progress"),
                    TaskQueueTaskInput("Open the PR with the numbers", "pending"),
                ),
                backgroundTasks = listOf(
                    TaskQueueBackgroundInput("b1", "shell", "Rerun the web and web-phone captures", startedAt = 0L),
                ),
                waits = emptyList(),
                now = 130_000L,
            ),
        )!!
    }
    TaskQueueView(
        queue = queue,
        todoOpen = todoOpen,
        onTodoOpenChange = { todoOpen = it },
        completedOpen = doneOpen,
        onCompletedOpenChange = { doneOpen = it },
    )
    Trace {
        TraceLine(TraceModels.tool("search", "Grep", "persistOnStop", settled = true, failed = false))
        TraceLine(TraceModels.tool("execute", "Bash", "bun test", settled = true, failed = true))
    }
}

@Composable
private fun CodeBlockSpecimen() {
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        CodeBlock(TS_SOURCE, title = "startup.ts")
        CodeBlock(ANSI_OUTPUT, language = CodeBlockLanguage.Ansi)
        CodeBlock(BENCHMARK_OUTPUT, language = CodeBlockLanguage.Ansi, destructive = true)
    }
}
