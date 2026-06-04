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
 * {"type": "...", ...}; `len` is its byte length. Borrowed for the call only. */
typedef void (*AgentCoreEventCallback)(void *ctx, const char *event_json, size_t len);

/* Lifecycle. config_json carries baseUrl, workspaceId, agentId, botUserId(expk_),
 * githubOauthClientId, statePath, worktreeRoot, reposRoot, driver(claude|codex). */
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

/* Agent-run bridge. The core emits a `run_request` event; the GUI launches
 * claude/codex inside its libghostty PTY and reports completion here.
 *
 * The run_request JSON carries `interactive` (bool) and `continueSessionId`
 * (string|null): when interactive, the host runs the CLI WITHOUT output capture
 * (the plan is delivered out-of-band via MCP) and should surface the session id.
 * `session_id` is the CLI session (for a later --continue); pass NULL when
 * unknown / for headless runs. */
int agent_core_submit_run_result(AgentCore *core, const char *run_id, int exit_code,
                                 const char *final_text, const char *session_id);
int agent_core_cancel_run(AgentCore *core, const char *run_id);

/* Host-triggered interactive sessions (desktop "AI" button / "Approve & continue
 * here"). Each returns immediately and runs on a worker thread; the host then
 * receives a `run_request` with interactive:true to launch in the terminal.
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
