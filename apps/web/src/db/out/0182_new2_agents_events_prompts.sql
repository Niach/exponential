CREATE TYPE "public"."event_emitter" AS ENUM('session', 'user', 'server');--> statement-breakpoint
CREATE TABLE "agents" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"name" varchar(64) NOT NULL,
	"icon" varchar(64),
	"color" varchar(7),
	"instructions" text DEFAULT '' NOT NULL,
	"triggers" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"trigger_state" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"device_id" varchar(128) NOT NULL,
	"cli" varchar(16),
	"account" varchar(64),
	"model" varchar(64),
	"owner_id" text NOT NULL,
	"sort_order" double precision DEFAULT 0 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"name" varchar(128) NOT NULL,
	"payload" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"emitted_by" "event_emitter" NOT NULL,
	"session_id" uuid,
	"user_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "prompts" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"name" varchar(255) NOT NULL,
	"icon" varchar(64),
	"body" text NOT NULL,
	"sort_order" double precision DEFAULT 0 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "prompts_team_id_name_unique" UNIQUE("team_id","name")
);
--> statement-breakpoint
ALTER TABLE "sessions" DROP CONSTRAINT "sessions_agent_root_check";--> statement-breakpoint
ALTER TABLE "agents" ADD CONSTRAINT "agents_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "agents" ADD CONSTRAINT "agents_owner_id_users_id_fk" FOREIGN KEY ("owner_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "events" ADD CONSTRAINT "events_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "events" ADD CONSTRAINT "events_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "events" ADD CONSTRAINT "events_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "prompts" ADD CONSTRAINT "prompts_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_agents_team" ON "agents" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_agents_device" ON "agents" USING btree ("device_id");--> statement-breakpoint
CREATE INDEX "idx_events_team_created" ON "events" USING btree ("team_id","created_at");--> statement-breakpoint
CREATE INDEX "idx_events_team_name" ON "events" USING btree ("team_id","name");--> statement-breakpoint
CREATE INDEX "idx_prompts_team" ON "prompts" USING btree ("team_id");--> statement-breakpoint
-- NEW-2 data: every team agent (a `sessions` root of kind agent) becomes an
-- `agents` definition with the SAME id (its URLs and pins keep resolving);
-- its triggers move to the cron + event-name vocabulary (daily/weekly/monthly
-- → a 5-field cron in UTC; created/status_changed/pr_opened/pr_merged → the
-- builtin event names; the other event kinds have no server builtin and are
-- dropped). 0183 then marks the old root as the owner's conversation session.
INSERT INTO "agents" ("id", "team_id", "name", "icon", "color", "instructions", "triggers", "device_id", "cli", "account", "model", "owner_id", "sort_order", "created_at", "updated_at")
SELECT s.id, s.team_id, coalesce(s.name, 'Agent'), s.icon, s.color, coalesce(s.instructions, ''),
  coalesce((
    SELECT jsonb_agg(
      CASE
        WHEN t->>'kind' = 'schedule' THEN jsonb_build_object(
          'id', coalesce(t->>'id', gen_random_uuid()::text),
          'enabled', coalesce((t->>'enabled')::boolean, true),
          'kind', 'schedule',
          'cron', (coalesce((t->>'minuteOfDay')::int, 0) % 60)::text || ' ' || (coalesce((t->>'minuteOfDay')::int, 0) / 60)::text || ' '
            || CASE t->>'interval' WHEN 'monthly' THEN coalesce(t->>'dayOfMonth', '1') ELSE '*' END
            || ' * '
            || CASE t->>'interval' WHEN 'weekly' THEN (coalesce((t->>'weekday')::int, 1) % 7)::text ELSE '*' END)
        ELSE jsonb_build_object(
          'id', coalesce(t->>'id', gen_random_uuid()::text),
          'enabled', coalesce((t->>'enabled')::boolean, true),
          'kind', 'event',
          'name', CASE t->>'event'
            WHEN 'created' THEN 'issue.created'
            WHEN 'status_changed' THEN 'issue.status_changed'
            WHEN 'pr_opened' THEN 'pr.opened'
            WHEN 'pr_merged' THEN 'pr.merged' END)
      END)
    FROM jsonb_array_elements(s.triggers) t
    WHERE t->>'kind' = 'schedule' OR t->>'event' IN ('created', 'status_changed', 'pr_opened', 'pr_merged')
  ), '[]'::jsonb),
  s.device_id, s.cli, s.account, s.model, s.user_id, s.sort_order, s.created_at, s.updated_at
FROM "sessions" s
WHERE s.kind = 'agent' AND s.parent_id IS NULL;--> statement-breakpoint
-- The ≤0.14.74 devices armed `sessions.triggers` themselves; the server
-- fires now, so the old rows stop carrying them.
UPDATE "sessions" SET "triggers" = '[]'::jsonb WHERE "kind" = 'agent';--> statement-breakpoint
-- Actions become prompts (name, icon, body; inputs dropped); the `actions`
-- table stays EMPTY as a read shim for the shipped clients (NEW-8 drops it).
INSERT INTO "prompts" ("id", "team_id", "name", "icon", "body", "sort_order", "created_at", "updated_at")
SELECT a.id, a.team_id, a.name, a.icon, a.body, a.sort_order, a.created_at, a.updated_at FROM "actions" a;--> statement-breakpoint
DELETE FROM "actions";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "instructions";