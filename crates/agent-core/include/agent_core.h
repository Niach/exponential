/*
 * agent_core.h — C ABI for the Exponential shared agent-loop core.
 *
 * Consumed by the macOS app (Swift, via a clang module map) and the Linux app
 * (Zig, via @cImport). The boundary is synchronous and thread-safe: every call
 * returns immediately; all real work runs on the core's own background runtime,
 * and results flow back exclusively through the single event callback.
 *
 * M0: hand-maintained stub. Will become cbindgen-generated once deps/network are
 * wired (see cbindgen.toml). Keep this in sync with crates/agent-core/src/ffi.rs.
 *
 * Memory: strings passed to the event callback are BORROWED — valid only for the
 * duration of the call; the host must copy them. Strings returned via `char**`
 * out-params are owned by the caller and must be released with
 * agent_core_string_free().
 */
#ifndef EXP_AGENT_CORE_H
#define EXP_AGENT_CORE_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle owning the core's runtime + state. */
typedef struct AgentCore AgentCore;

/* Single outbound-event sink. `event_json` is a UTF-8 JSON object
 * {"type": "...", ...}; `len` is its byte length. Borrowed for the call only.
 *
 * Event types:
 *   run_request   — demands a response via agent_core_submit_run_result. Fields:
 *                   runId, issueId, issueIdentifier, cwd, mode(plan|code),
 *                   program, argv[], env{}, mcpConfigPath, systemPrompt,
 *                   userPrompt, interactive(bool), continueSessionId(string|null).
 *   run_started   — {issueId, issueIdentifier, runId, mode} (toast / indicator).
 *   run_finished  — {issueId, runId, exitCode, outcome: ok|failed|cancelled}.
 *   run_cancelled — {issueId(string|null), runId} → tear down the matching
 *                   terminal (destroying the surface kills the CLI child).
 *   agent_error   — {issueId, code, message}; stable codes include
 *                   repo_not_linked, repo_token_unavailable, plan_not_submitted,
 *                   no_commits, pipeline_failed, interactive_failed, plus the
 *                   informational rejections interactive_session_active /
 *                   run_already_in_flight (a trigger was refused because a
 *                   session/run is already live — nothing failed).
 *   log           — {level, message}. */
typedef void (*AgentCoreEventCallback)(void *ctx, const char *event_json, size_t len);

/* Lifecycle. config_json (camelCase, see CoreConfigDto in ffi.rs) carries:
 * baseUrl, apiKey(expk_), botUserId, githubToken?, reposRoot, worktreesRoot,
 * branchPrefix("agent"), driver(claude|codex), dbPath, maxConcurrent(2),
 * timeoutS(30), runTimeoutS(1800, headless wall-clock cap), interactive(true —
 * route the claude plan/code stages through the host terminal as live
 * sessions; headless runs execute IN-CORE and never reach the host). */
AgentCore *agent_core_create(const char *config_json);
void agent_core_set_event_callback(AgentCore *core, void *ctx, AgentCoreEventCallback cb);
int agent_core_start(AgentCore *core);
int agent_core_stop(AgentCore *core);
void agent_core_free(AgentCore *core);

/* Setup / identity. Wraps companion.claimSetup; on success *out_json receives an
 * owned JSON string {apiKey, agent, workspace, projects, oauth} (free it). */
int agent_core_claim_setup(const char *base_url, const char *setup_token, char **out_json);
int agent_core_github_device_login(AgentCore *core); /* emits github_device_prompt */
int agent_core_uninstall(AgentCore *core);           /* companion.uninstallSelf */

/* Agent-run bridge. For INTERACTIVE runs the core emits `run_request` and the
 * GUI launches claude inside its libghostty PTY, reporting the exit here (the
 * plan/code results are verified out-of-band — final_text may be empty).
 * Headless runs execute in-core and never reach the host.
 *
 * `session_id` may be NULL: the core pins session identity itself via
 * --session-id/--resume and only falls back to log recovery without it. */
int agent_core_submit_run_result(AgentCore *core, const char *run_id, int exit_code,
                                 const char *final_text, const char *session_id);
int agent_core_cancel_run(AgentCore *core, const char *run_id);

/* Cancel the run currently in flight for an issue (the desktop "Cancel" button).
 * The host passes the issue id; the core maps it to the in-flight run and drops
 * it, unblocking the parked pipeline so the issue stops running. No-op if nothing
 * is in flight for that issue. */
int agent_core_cancel_issue(AgentCore *core, const char *issue_id);

/* Host-triggered interactive sessions (desktop "AI" button / "Approve & continue
 * here"). Each returns immediately and runs on a worker thread; the host then
 * receives a `run_request` with interactive:true to launch in the terminal.
 * At most ONE interactive session is live at a time (the dock holds one
 * terminal): while one is mounted — host-triggered or dispatcher-driven — these
 * calls are refused with an agent_error(interactive_session_active) event and
 * emit no run_request (the mounted session is untouched).
 * `agent_core_approve_interactive` assumes the host already approved the plan
 * with the human's session — it only resumes the session to implement it. */
int agent_core_request_interactive(AgentCore *core, const char *issue_id);
int agent_core_approve_interactive(AgentCore *core, const char *issue_id);

/* Release a string returned via a char** out-param. */
void agent_core_string_free(char *s);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* EXP_AGENT_CORE_H */
