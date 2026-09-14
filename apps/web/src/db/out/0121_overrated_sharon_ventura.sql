CREATE TABLE "issue_drafts" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" text NOT NULL,
	"team_id" uuid NOT NULL,
	"board_id" uuid NOT NULL,
	"title" text DEFAULT '' NOT NULL,
	"description" text DEFAULT '' NOT NULL,
	"status_id" uuid,
	"priority" "issue_priority" DEFAULT 'none' NOT NULL,
	"assignee_id" text,
	"label_ids" uuid[] DEFAULT '{}'::uuid[] NOT NULL,
	"due_date" date,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "attachments" ALTER COLUMN "issue_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "attachments" ALTER COLUMN "board_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "attachments" ADD COLUMN "draft_id" uuid;--> statement-breakpoint
ALTER TABLE "issue_drafts" ADD CONSTRAINT "issue_drafts_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_drafts" ADD CONSTRAINT "issue_drafts_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_drafts" ADD CONSTRAINT "issue_drafts_board_id_boards_id_fk" FOREIGN KEY ("board_id") REFERENCES "public"."boards"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_drafts" ADD CONSTRAINT "issue_drafts_status_id_issue_statuses_id_fk" FOREIGN KEY ("status_id") REFERENCES "public"."issue_statuses"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_drafts" ADD CONSTRAINT "issue_drafts_assignee_id_users_id_fk" FOREIGN KEY ("assignee_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_issue_drafts_user" ON "issue_drafts" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "idx_issue_drafts_team" ON "issue_drafts" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_issue_drafts_board" ON "issue_drafts" USING btree ("board_id");--> statement-breakpoint
ALTER TABLE "attachments" ADD CONSTRAINT "attachments_draft_id_issue_drafts_id_fk" FOREIGN KEY ("draft_id") REFERENCES "public"."issue_drafts"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_attachments_draft" ON "attachments" USING btree ("draft_id") WHERE draft_id IS NOT NULL;--> statement-breakpoint
ALTER TABLE "attachments" ADD CONSTRAINT "attachments_owner_check" CHECK ((issue_id IS NOT NULL) <> (draft_id IS NOT NULL));