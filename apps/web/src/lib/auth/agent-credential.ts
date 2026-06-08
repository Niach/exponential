import { auth } from "@/lib/auth"

export interface AgentApiKey {
  // The plaintext expk_… key. Returned exactly once at mint time; the apikeys
  // row stores only a hash, so the device must persist this.
  key: string
  // apikeys.id — stored on agent_registrations.apiKeyId for revocation.
  keyId: string
}

// Mints the runtime credential for a desktop device: a single long-lived
// `expk_` API key bound to the device's synthetic agent user. The device sends
// it as `Authorization: Bearer expk_…`; the Better Auth apiKey() plugin resolves
// it back to the agent user (enableSessionForAPIKeys), so the device acts as a
// normal authenticated principal — no OAuth client, no refresh-token rotation,
// no MCP token endpoint. Keys are non-expiring (keyExpiration.defaultExpiresIn
// is null) and revoked by deleting the apikeys row.
export async function mintAgentApiKey(args: {
  agentUserId: string
  deviceName: string
  deviceId: string
}): Promise<AgentApiKey> {
  const apiKey = await auth.api.createApiKey({
    body: {
      name: `Device: ${args.deviceName}`,
      userId: args.agentUserId,
      expiresIn: null,
      rateLimitEnabled: false,
      metadata: { kind: `device`, deviceId: args.deviceId },
    },
  })
  return { key: apiKey.key, keyId: apiKey.id }
}
