ALTER TABLE "attachments" DROP CONSTRAINT "attachments_uploader_id_users_id_fk";
--> statement-breakpoint
ALTER TABLE "creem_subscriptions" DROP CONSTRAINT "creem_subscriptions_reference_id_users_id_fk";
--> statement-breakpoint
ALTER TABLE "creem_subscriptions" ALTER COLUMN "reference_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "attachments" ADD CONSTRAINT "attachments_uploader_id_users_id_fk" FOREIGN KEY ("uploader_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "creem_subscriptions" ADD CONSTRAINT "creem_subscriptions_reference_id_users_id_fk" FOREIGN KEY ("reference_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;