ALTER TABLE "sessions" ADD COLUMN "dispatch" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "work_launch" jsonb;--> statement-breakpoint
-- Owner 2026-10-15: every model is explicit. A device that saved a blank
-- ("CLI default") model or subagent model gets the contract defaults:
-- claude model fable, subagents opus; codex model gpt-5.6-sol.
UPDATE "devices" SET "launch_defaults" = jsonb_set("launch_defaults", '{agents,claude,subagentModel}', '"opus"')
WHERE "launch_defaults" #>> '{agents,claude,subagentModel}' = '';--> statement-breakpoint
UPDATE "devices" SET "launch_defaults" = jsonb_set("launch_defaults", '{agents,claude,model}', '"fable"')
WHERE "launch_defaults" #>> '{agents,claude,model}' = '';--> statement-breakpoint
UPDATE "devices" SET "launch_defaults" = jsonb_set("launch_defaults", '{agents,codex,model}', '"gpt-5.6-sol"')
WHERE "launch_defaults" #>> '{agents,codex,model}' = '';
