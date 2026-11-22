ALTER TABLE "pull_requests" ADD COLUMN "merge_now_by" text;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "merge_now_red" jsonb;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD CONSTRAINT "pull_requests_merge_now_by_users_id_fk" FOREIGN KEY ("merge_now_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;