/*
 * Minimal C harness proving the agent-core C ABI is consumable end-to-end —
 * exactly what the macOS (Swift) and Linux (Zig) hosts do under the hood:
 * create → register a callback → start → receive events → stop → free.
 *
 * Build & run (from repo root, after `cargo build -p agent-core`):
 *   cc crates/agent-core/examples/smoke.c \
 *      -I crates/agent-core/include -L target/debug -lagent_core \
 *      -o /tmp/agentcore_smoke
 *   LD_LIBRARY_PATH=target/debug /tmp/agentcore_smoke
 */
#include "agent_core.h"
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static int event_count = 0;

static void on_event(void *ctx, const char *event_json, size_t len) {
  (void)ctx;
  (void)len;
  event_count++;
  printf("event: %s\n", event_json); /* borrowed string — print immediately */
}

int main(void) {
  AgentCore *core = agent_core_create("{\"baseUrl\":\"http://localhost:5173\"}");
  if (!core) {
    fprintf(stderr, "create failed\n");
    return 1;
  }
  agent_core_set_event_callback(core, NULL, on_event);

  if (agent_core_start(core) != 0) {
    fprintf(stderr, "start failed\n");
    return 1;
  }
  usleep(150 * 1000); /* let the startup log + a heartbeat or two arrive */
  agent_core_stop(core);
  agent_core_free(core);

  printf("received %d event(s)\n", event_count);
  return event_count >= 1 ? 0 : 2;
}
