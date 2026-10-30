ALTER TABLE "messages" ADD COLUMN "attachment_ids" uuid[];--> statement-breakpoint
ALTER TABLE "messages" ADD COLUMN "mention_user_ids" text[];--> statement-breakpoint
ALTER TABLE "messages" ADD COLUMN "ref_issue_ids" uuid[];