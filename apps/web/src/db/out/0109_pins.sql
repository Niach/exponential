CREATE TYPE "public"."pin_kind" AS ENUM('issue', 'session', 'action');--> statement-breakpoint
CREATE TABLE "pins" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" text NOT NULL,
	"team_id" uuid NOT NULL,
	"kind" "pin_kind" NOT NULL,
	"issue_id" uuid,
	"session_id" uuid,
	"action_id" uuid,
	"sort_order" double precision DEFAULT 0 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "pins_kind_target_check" CHECK ((kind = 'issue' AND issue_id IS NOT NULL AND session_id IS NULL AND action_id IS NULL) OR (kind = 'session' AND session_id IS NOT NULL AND issue_id IS NULL AND action_id IS NULL) OR (kind = 'action' AND action_id IS NOT NULL AND issue_id IS NULL AND session_id IS NULL))
);
--> statement-breakpoint
ALTER TABLE "pins" ADD CONSTRAINT "pins_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pins" ADD CONSTRAINT "pins_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pins" ADD CONSTRAINT "pins_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pins" ADD CONSTRAINT "pins_session_id_coding_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."coding_sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pins" ADD CONSTRAINT "pins_action_id_actions_id_fk" FOREIGN KEY ("action_id") REFERENCES "public"."actions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_pins_user_issue" ON "pins" USING btree ("user_id","issue_id") WHERE issue_id IS NOT NULL;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_pins_user_session" ON "pins" USING btree ("user_id","session_id") WHERE session_id IS NOT NULL;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_pins_user_action" ON "pins" USING btree ("user_id","action_id") WHERE action_id IS NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_pins_user" ON "pins" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "idx_pins_team" ON "pins" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_pins_issue" ON "pins" USING btree ("issue_id");--> statement-breakpoint
CREATE INDEX "idx_pins_session" ON "pins" USING btree ("session_id");--> statement-breakpoint
CREATE INDEX "idx_pins_action" ON "pins" USING btree ("action_id");