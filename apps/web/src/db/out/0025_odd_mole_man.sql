ALTER TYPE "public"."notification_type" ADD VALUE 'support_reply';--> statement-breakpoint
CREATE TABLE "support_messages" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"thread_id" uuid NOT NULL,
	"issue_id" uuid NOT NULL,
	"author_user_id" text,
	"direction" varchar(16) NOT NULL,
	"visibility" varchar(16) DEFAULT 'public' NOT NULL,
	"body" text NOT NULL,
	"email_delivery_id" uuid,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "support_threads" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"issue_id" uuid NOT NULL,
	"project_id" uuid NOT NULL,
	"reporter_email" varchar(320) NOT NULL,
	"reporter_name" varchar(255),
	"token" varchar(64) NOT NULL,
	"token_revoked_at" timestamp with time zone,
	"last_reporter_seen_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "support_threads_issue_id_unique" UNIQUE("issue_id"),
	CONSTRAINT "support_threads_token_unique" UNIQUE("token")
);
--> statement-breakpoint
ALTER TABLE "projects" ADD COLUMN "helpdesk_enabled" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "support_messages" ADD CONSTRAINT "support_messages_thread_id_support_threads_id_fk" FOREIGN KEY ("thread_id") REFERENCES "public"."support_threads"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "support_messages" ADD CONSTRAINT "support_messages_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "support_messages" ADD CONSTRAINT "support_messages_author_user_id_users_id_fk" FOREIGN KEY ("author_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "support_messages" ADD CONSTRAINT "support_messages_email_delivery_id_email_deliveries_id_fk" FOREIGN KEY ("email_delivery_id") REFERENCES "public"."email_deliveries"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "support_threads" ADD CONSTRAINT "support_threads_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "support_threads" ADD CONSTRAINT "support_threads_project_id_projects_id_fk" FOREIGN KEY ("project_id") REFERENCES "public"."projects"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_support_messages_thread" ON "support_messages" USING btree ("thread_id");--> statement-breakpoint
CREATE INDEX "idx_support_messages_issue" ON "support_messages" USING btree ("issue_id");--> statement-breakpoint
CREATE INDEX "idx_support_threads_project" ON "support_threads" USING btree ("project_id");