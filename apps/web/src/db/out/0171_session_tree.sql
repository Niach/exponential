CREATE TYPE "public"."message_author_kind" AS ENUM('user', 'agent', 'system');--> statement-breakpoint
CREATE TYPE "public"."session_issue_source" AS ENUM('user', 'agent');--> statement-breakpoint
CREATE TYPE "public"."session_kind" AS ENUM('thread', 'agent');--> statement-breakpoint
CREATE TYPE "public"."session_status" AS ENUM('queued', 'running', 'idle', 'ended');--> statement-breakpoint
CREATE TABLE "messages" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"session_id" uuid NOT NULL,
	"root_id" uuid NOT NULL,
	"team_id" uuid NOT NULL,
	"shared" boolean DEFAULT false NOT NULL,
	"owner_id" text,
	"author_kind" "message_author_kind" NOT NULL,
	"author_id" text,
	"body" text DEFAULT '' NOT NULL,
	"card" jsonb,
	"answered_at" timestamp with time zone,
	"consumed_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "pull_request_issues" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"pull_request_id" uuid NOT NULL,
	"issue_id" uuid NOT NULL,
	"team_id" uuid NOT NULL,
	"board_id" uuid,
	"board_deleted_at" timestamp with time zone,
	"board_archived_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "pull_request_issues_pull_request_id_issue_id_unique" UNIQUE("pull_request_id","issue_id")
);
--> statement-breakpoint
CREATE TABLE "pull_requests" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"root_session_id" uuid,
	"session_id" uuid,
	"repository_id" uuid NOT NULL,
	"number" integer NOT NULL,
	"url" text NOT NULL,
	"title" varchar(500),
	"head" varchar(255) NOT NULL,
	"base" varchar(255) NOT NULL,
	"base_pull_request_id" uuid,
	"state" "pr_state" DEFAULT 'open' NOT NULL,
	"merged_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "pull_requests_repository_id_number_unique" UNIQUE("repository_id","number")
);
--> statement-breakpoint
CREATE TABLE "session_issues" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"session_id" uuid NOT NULL,
	"issue_id" uuid NOT NULL,
	"team_id" uuid NOT NULL,
	"board_id" uuid,
	"board_deleted_at" timestamp with time zone,
	"board_archived_at" timestamp with time zone,
	"shared" boolean DEFAULT false NOT NULL,
	"owner_id" text,
	"source" "session_issue_source" DEFAULT 'user' NOT NULL,
	"added_by" text,
	"added_by_session_id" uuid,
	"sort_order" double precision DEFAULT 0 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "session_issues_session_id_issue_id_unique" UNIQUE("session_id","issue_id")
);
--> statement-breakpoint
CREATE TABLE "sessions" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"parent_id" uuid,
	"root_id" uuid NOT NULL,
	"depth" integer DEFAULT 0 NOT NULL,
	"kind" "session_kind" DEFAULT 'thread' NOT NULL,
	"user_id" text NOT NULL,
	"host_user_id" text,
	"device_id" varchar(128) NOT NULL,
	"cli" varchar(16),
	"account" varchar(64),
	"model" varchar(64),
	"effort" varchar(32),
	"plan_mode" boolean DEFAULT false NOT NULL,
	"computer_use" boolean DEFAULT false NOT NULL,
	"ultracode" boolean DEFAULT false NOT NULL,
	"mcp_server_ids" jsonb,
	"repository_id" uuid,
	"branch" varchar(255),
	"status" "session_status" DEFAULT 'queued' NOT NULL,
	"busy" boolean DEFAULT false NOT NULL,
	"needs_input" boolean DEFAULT false NOT NULL,
	"blocked" jsonb,
	"started_at" timestamp with time zone,
	"last_turn_at" timestamp with time zone,
	"ended_at" timestamp with time zone,
	"shared" boolean DEFAULT false NOT NULL,
	"title" varchar(255),
	"guide" jsonb,
	"action_id" uuid,
	"action_name" varchar(255),
	"action_inputs" jsonb,
	"spawned_by_session_id" uuid,
	"name" varchar(64),
	"icon" varchar(64),
	"color" varchar(7),
	"instructions" text,
	"triggers" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"sort_order" double precision DEFAULT 0 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "sessions_agent_root_check" CHECK (kind = 'thread' OR (parent_id IS NULL AND name IS NOT NULL)),
	CONSTRAINT "sessions_root_check" CHECK ((parent_id IS NULL AND depth = 0 AND root_id = id) OR (parent_id IS NOT NULL AND depth > 0))
);
--> statement-breakpoint
ALTER TABLE "attachments" DROP CONSTRAINT "attachments_owner_check";--> statement-breakpoint
ALTER TABLE "attachments" ADD COLUMN "message_id" uuid;--> statement-breakpoint
ALTER TABLE "messages" ADD CONSTRAINT "messages_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "messages" ADD CONSTRAINT "messages_root_id_sessions_id_fk" FOREIGN KEY ("root_id") REFERENCES "public"."sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "messages" ADD CONSTRAINT "messages_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "messages" ADD CONSTRAINT "messages_owner_id_users_id_fk" FOREIGN KEY ("owner_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "messages" ADD CONSTRAINT "messages_author_id_users_id_fk" FOREIGN KEY ("author_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pull_request_issues" ADD CONSTRAINT "pull_request_issues_pull_request_id_pull_requests_id_fk" FOREIGN KEY ("pull_request_id") REFERENCES "public"."pull_requests"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pull_request_issues" ADD CONSTRAINT "pull_request_issues_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pull_request_issues" ADD CONSTRAINT "pull_request_issues_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pull_request_issues" ADD CONSTRAINT "pull_request_issues_board_id_boards_id_fk" FOREIGN KEY ("board_id") REFERENCES "public"."boards"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD CONSTRAINT "pull_requests_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD CONSTRAINT "pull_requests_root_session_id_sessions_id_fk" FOREIGN KEY ("root_session_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD CONSTRAINT "pull_requests_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD CONSTRAINT "pull_requests_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD CONSTRAINT "pull_requests_base_pull_request_id_pull_requests_id_fk" FOREIGN KEY ("base_pull_request_id") REFERENCES "public"."pull_requests"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session_issues" ADD CONSTRAINT "session_issues_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session_issues" ADD CONSTRAINT "session_issues_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session_issues" ADD CONSTRAINT "session_issues_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session_issues" ADD CONSTRAINT "session_issues_board_id_boards_id_fk" FOREIGN KEY ("board_id") REFERENCES "public"."boards"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session_issues" ADD CONSTRAINT "session_issues_owner_id_users_id_fk" FOREIGN KEY ("owner_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session_issues" ADD CONSTRAINT "session_issues_added_by_users_id_fk" FOREIGN KEY ("added_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session_issues" ADD CONSTRAINT "session_issues_added_by_session_id_sessions_id_fk" FOREIGN KEY ("added_by_session_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_parent_id_sessions_id_fk" FOREIGN KEY ("parent_id") REFERENCES "public"."sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_root_id_sessions_id_fk" FOREIGN KEY ("root_id") REFERENCES "public"."sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_host_user_id_users_id_fk" FOREIGN KEY ("host_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_action_id_actions_id_fk" FOREIGN KEY ("action_id") REFERENCES "public"."actions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_spawned_by_session_id_sessions_id_fk" FOREIGN KEY ("spawned_by_session_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_messages_session_created" ON "messages" USING btree ("session_id","created_at");--> statement-breakpoint
CREATE INDEX "idx_messages_root" ON "messages" USING btree ("root_id");--> statement-breakpoint
CREATE INDEX "idx_messages_team" ON "messages" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_messages_pending" ON "messages" USING btree ("session_id") WHERE author_kind = 'user' AND consumed_at IS NULL;--> statement-breakpoint
CREATE INDEX "idx_pull_request_issues_issue" ON "pull_request_issues" USING btree ("issue_id");--> statement-breakpoint
CREATE INDEX "idx_pull_request_issues_team" ON "pull_request_issues" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_pull_request_issues_board" ON "pull_request_issues" USING btree ("board_id");--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_pull_requests_url" ON "pull_requests" USING btree ("url");--> statement-breakpoint
CREATE INDEX "idx_pull_requests_team" ON "pull_requests" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_pull_requests_root" ON "pull_requests" USING btree ("root_session_id");--> statement-breakpoint
CREATE INDEX "idx_pull_requests_session" ON "pull_requests" USING btree ("session_id");--> statement-breakpoint
CREATE INDEX "idx_pull_requests_head" ON "pull_requests" USING btree ("repository_id","head");--> statement-breakpoint
CREATE INDEX "idx_pull_requests_base_pr" ON "pull_requests" USING btree ("base_pull_request_id");--> statement-breakpoint
CREATE INDEX "idx_session_issues_issue" ON "session_issues" USING btree ("issue_id");--> statement-breakpoint
CREATE INDEX "idx_session_issues_team" ON "session_issues" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_session_issues_board" ON "session_issues" USING btree ("board_id");--> statement-breakpoint
CREATE INDEX "idx_session_issues_added_by_session" ON "session_issues" USING btree ("added_by_session_id");--> statement-breakpoint
CREATE INDEX "idx_sessions_team" ON "sessions" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_sessions_parent" ON "sessions" USING btree ("parent_id");--> statement-breakpoint
CREATE INDEX "idx_sessions_root" ON "sessions" USING btree ("root_id");--> statement-breakpoint
CREATE INDEX "idx_sessions_user" ON "sessions" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "idx_sessions_action" ON "sessions" USING btree ("action_id");--> statement-breakpoint
CREATE INDEX "idx_sessions_spawned_by" ON "sessions" USING btree ("spawned_by_session_id");--> statement-breakpoint
CREATE INDEX "idx_sessions_device_live" ON "sessions" USING btree ("device_id","status") WHERE status <> 'ended';--> statement-breakpoint
ALTER TABLE "attachments" ADD CONSTRAINT "attachments_message_id_messages_id_fk" FOREIGN KEY ("message_id") REFERENCES "public"."messages"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_attachments_message" ON "attachments" USING btree ("message_id") WHERE message_id IS NOT NULL;--> statement-breakpoint
ALTER TABLE "attachments" ADD CONSTRAINT "attachments_owner_check" CHECK (num_nonnulls(issue_id, draft_id, message_id) <= 1);