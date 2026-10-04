// The plugin is manifest-only: `openclaw.plugin.json` contributes the
// `exponential` MCP server and the skill. OpenClaw still loads a runtime
// entry for every native plugin, so this is the empty one — the same shape
// `definePluginEntry` returns, without depending on the SDK.
export default {
  id: "exponential",
  name: "Exponential",
  description: "Exponential's issue tracker over MCP, with MCP Apps views.",
  configSchema: {
    safeParse(value) {
      if (value === undefined) return { success: true, data: undefined };
      if (!value || typeof value !== "object" || Array.isArray(value)) {
        return { success: false, error: { issues: [{ path: [], message: "expected config object" }] } };
      }
      if (Object.keys(value).length > 0) {
        return { success: false, error: { issues: [{ path: [], message: "config must be empty" }] } };
      }
      return { success: true, data: value };
    },
    jsonSchema: { type: "object", additionalProperties: false, properties: {} },
  },
  register() {},
};
