CREATE TABLE "device_commands" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"device_row_id" uuid NOT NULL,
	"user_id" text NOT NULL,
	"kind" varchar(32) NOT NULL,
	"payload" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"status" varchar(16) DEFAULT 'pending' NOT NULL,
	"result" text,
	"completed_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "device_worktrees" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"device_row_id" uuid NOT NULL,
	"user_id" text NOT NULL,
	"shared_team_id" uuid,
	"repo_full_name" varchar(255) NOT NULL,
	"branch" varchar(255) NOT NULL,
	"issue_identifier" varchar(64),
	"agents" jsonb,
	"dirty" varchar(32) DEFAULT 'unknown' NOT NULL,
	"busy" boolean DEFAULT false NOT NULL,
	"reported_at" timestamp with time zone DEFAULT now() NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "device_worktrees_device_row_id_repo_full_name_branch_unique" UNIQUE("device_row_id","repo_full_name","branch")
);
--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "unauthed_agents" jsonb DEFAULT '[]'::jsonb NOT NULL;--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "launch_defaults" jsonb;--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "launch_defaults_updated_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "device_commands" ADD CONSTRAINT "device_commands_device_row_id_devices_id_fk" FOREIGN KEY ("device_row_id") REFERENCES "public"."devices"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "device_commands" ADD CONSTRAINT "device_commands_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "device_worktrees" ADD CONSTRAINT "device_worktrees_device_row_id_devices_id_fk" FOREIGN KEY ("device_row_id") REFERENCES "public"."devices"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "device_worktrees" ADD CONSTRAINT "device_worktrees_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "device_worktrees" ADD CONSTRAINT "device_worktrees_shared_team_id_teams_id_fk" FOREIGN KEY ("shared_team_id") REFERENCES "public"."teams"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_device_commands_pending" ON "device_commands" USING btree ("device_row_id") WHERE status = 'pending';--> statement-breakpoint
CREATE INDEX "idx_device_worktrees_user" ON "device_worktrees" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "idx_device_worktrees_shared_team" ON "device_worktrees" USING btree ("shared_team_id") WHERE shared_team_id IS NOT NULL;