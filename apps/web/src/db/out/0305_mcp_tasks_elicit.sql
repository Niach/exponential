CREATE TABLE "mcp_elicitations" (
	"id" varchar(64) PRIMARY KEY NOT NULL,
	"purpose" varchar(32) NOT NULL,
	"user_id" text NOT NULL,
	"grant" varchar(160) NOT NULL,
	"target" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"opened_at" timestamp with time zone,
	"expires_at" timestamp with time zone NOT NULL
);
--> statement-breakpoint
CREATE TABLE "mcp_request_states" (
	"jti" varchar(64) PRIMARY KEY NOT NULL,
	"expires_at" timestamp with time zone NOT NULL
);
--> statement-breakpoint
CREATE TABLE "mcp_tasks" (
	"id" varchar(64) PRIMARY KEY NOT NULL,
	"user_id" text NOT NULL,
	"grant" varchar(160) NOT NULL,
	"principal" varchar(256) NOT NULL,
	"team_id" uuid,
	"kind" varchar(16) NOT NULL,
	"ref" uuid,
	"status" varchar(24) DEFAULT 'working' NOT NULL,
	"status_message" text,
	"result" jsonb,
	"error" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"last_updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	"expires_at" timestamp with time zone NOT NULL
);
--> statement-breakpoint
ALTER TABLE "session_watches" DROP CONSTRAINT "uniq_session_watches_same";--> statement-breakpoint
ALTER TABLE "session_watches" ALTER COLUMN "session_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "mcp_calls" ADD COLUMN "method" varchar(64) DEFAULT 'tools/call' NOT NULL;--> statement-breakpoint
ALTER TABLE "session_watches" ADD COLUMN "watcher_kind" varchar(16) DEFAULT 'session' NOT NULL;--> statement-breakpoint
ALTER TABLE "session_watches" ADD COLUMN "task_id" varchar(64);--> statement-breakpoint
ALTER TABLE "mcp_elicitations" ADD CONSTRAINT "mcp_elicitations_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_tasks" ADD CONSTRAINT "mcp_tasks_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_tasks" ADD CONSTRAINT "mcp_tasks_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_mcp_elicitations_expires" ON "mcp_elicitations" USING btree ("expires_at");--> statement-breakpoint
CREATE INDEX "idx_mcp_request_states_expires" ON "mcp_request_states" USING btree ("expires_at");--> statement-breakpoint
CREATE INDEX "idx_mcp_tasks_principal" ON "mcp_tasks" USING btree ("principal","status");--> statement-breakpoint
CREATE INDEX "idx_mcp_tasks_ref" ON "mcp_tasks" USING btree ("ref");--> statement-breakpoint
CREATE INDEX "idx_mcp_tasks_expires" ON "mcp_tasks" USING btree ("expires_at");--> statement-breakpoint
ALTER TABLE "session_watches" ADD CONSTRAINT "session_watches_task_id_mcp_tasks_id_fk" FOREIGN KEY ("task_id") REFERENCES "public"."mcp_tasks"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_session_watches_task" ON "session_watches" USING btree ("task_id");--> statement-breakpoint
ALTER TABLE "session_watches" ADD CONSTRAINT "uniq_session_watches_same" UNIQUE NULLS NOT DISTINCT("session_id","task_id","kind","subject_id","status","at");--> statement-breakpoint
ALTER TABLE "session_watches" ADD CONSTRAINT "session_watches_one_watcher" CHECK ((watcher_kind = 'session' AND session_id IS NOT NULL AND task_id IS NULL) OR (watcher_kind = 'mcp_task' AND task_id IS NOT NULL AND session_id IS NULL));