-- EXP-849 retired the `pi` agent from contract `codingAgent`, but every
-- desktop/daemon below the version floor kept heart-beating it, so live rows
-- still carry `pi` in the devices registry (and in automations bound to it).
-- The router now clamps every write (lib/trpc/devices.ts `clampAgentIds` /
-- `clampAgentAccounts` / `clampAgentUsage` / `clampLaunchDefaults`); this is
-- the one-off heal for what landed before it. Data-only, no schema change.
--
-- Every statement is NULL-safe (the jsonb columns are nullable) and guarded,
-- so it touches only rows that actually name the retired agent — and it is
-- idempotent: a re-run matches nothing.

-- The three agent-id ARRAYS. `jsonb_agg` over the surviving elements, with
-- `coalesce` so the last element leaving yields `[]`, never NULL.
UPDATE "devices" SET "agents" = (
  SELECT coalesce(jsonb_agg(a), '[]'::jsonb)
  FROM jsonb_array_elements("agents") a
  WHERE a <> '"pi"'::jsonb
) WHERE "agents" @> '["pi"]'::jsonb;--> statement-breakpoint

UPDATE "devices" SET "acp_agents" = (
  SELECT coalesce(jsonb_agg(a), '[]'::jsonb)
  FROM jsonb_array_elements("acp_agents") a
  WHERE a <> '"pi"'::jsonb
) WHERE "acp_agents" @> '["pi"]'::jsonb;--> statement-breakpoint

UPDATE "devices" SET "unauthed_agents" = (
  SELECT coalesce(jsonb_agg(a), '[]'::jsonb)
  FROM jsonb_array_elements("unauthed_agents") a
  WHERE a <> '"pi"'::jsonb
) WHERE "unauthed_agents" @> '["pi"]'::jsonb;--> statement-breakpoint

-- The two per-agent MAPS (sign-in status + usage windows).
UPDATE "devices" SET "agent_accounts" = "agent_accounts" - 'pi'
WHERE jsonb_exists("agent_accounts", 'pi');--> statement-breakpoint

UPDATE "devices" SET "agent_usage" = "agent_usage" - 'pi'
WHERE jsonb_exists("agent_usage", 'pi');--> statement-breakpoint

-- Launch defaults: the per-agent entry, then an `agents` object it emptied
-- (the clamp writes no empty object), then a `defaultAgent` naming it — a
-- machine whose default was `pi` ends with NO default, which every client
-- already handles as "seed statically".
UPDATE "devices" SET "launch_defaults" = "launch_defaults" #- '{agents,pi}'
WHERE jsonb_exists("launch_defaults" -> 'agents', 'pi');--> statement-breakpoint

UPDATE "devices" SET "launch_defaults" = "launch_defaults" - 'agents'
WHERE "launch_defaults" -> 'agents' = '{}'::jsonb;--> statement-breakpoint

UPDATE "devices" SET "launch_defaults" = "launch_defaults" - 'defaultAgent'
WHERE "launch_defaults" ->> 'defaultAgent' = 'pi';--> statement-breakpoint

-- An automation pinned to the retired agent falls back to the action's /
-- device's default agent (NULL = "whatever the runner picks"), instead of
-- selecting a CLI that no longer exists on the machine.
UPDATE "automations" SET "agent" = NULL WHERE "agent" = 'pi';
