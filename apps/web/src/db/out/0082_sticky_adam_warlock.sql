CREATE TABLE "automations" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"action_id" uuid NOT NULL,
	"device_id" varchar(128) NOT NULL,
	"enabled" boolean DEFAULT true NOT NULL,
	"trigger" jsonb NOT NULL,
	"agent" varchar(16),
	"model" varchar(64),
	"effort" varchar(32),
	"sort_order" double precision DEFAULT 0 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "automation_id" uuid;--> statement-breakpoint
ALTER TABLE "automations" ADD CONSTRAINT "automations_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "automations" ADD CONSTRAINT "automations_action_id_actions_id_fk" FOREIGN KEY ("action_id") REFERENCES "public"."actions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_automations_team" ON "automations" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_automations_action" ON "automations" USING btree ("action_id");--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD CONSTRAINT "coding_sessions_automation_id_automations_id_fk" FOREIGN KEY ("automation_id") REFERENCES "public"."automations"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
-- EXP-583 backfill: every EXP-530 action trigger becomes its own automation
-- row (device/enabled split out of the jsonb; agent/model/effort NULL = device
-- defaults). Unparseable legacy rows are skipped, matching the clients' tolerant
-- read. Idempotent under the migrator's single-run guarantee.
INSERT INTO "automations" ("team_id", "action_id", "device_id", "enabled", "trigger", "sort_order", "created_at", "updated_at")
SELECT
	a."team_id",
	a."id",
	a."trigger"->>'deviceId',
	COALESCE((a."trigger"->>'enabled')::boolean, true),
	a."trigger" - 'deviceId' - 'enabled',
	a."sort_order",
	a."updated_at",
	a."updated_at"
FROM "actions" a
WHERE jsonb_typeof(a."trigger") = 'object'
	AND a."trigger"->>'kind' IN ('schedule', 'event')
	AND COALESCE(a."trigger"->>'deviceId', '') <> '';--> statement-breakpoint
ALTER TABLE "actions" DROP COLUMN "trigger";