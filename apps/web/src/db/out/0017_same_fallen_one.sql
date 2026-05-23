ALTER TABLE "comments" ADD COLUMN "kind" varchar(16) DEFAULT 'regular' NOT NULL;--> statement-breakpoint
ALTER TABLE "comments" ADD COLUMN "answered_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "agent_plan" text;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "agent_plan_state" varchar(32);--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "agent_plan_revision" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "agent_plan_approved_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "agent_plan_approved_by" text;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "agent_last_comment_seen_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_agent_plan_approved_by_users_id_fk" FOREIGN KEY ("agent_plan_approved_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;