ALTER TABLE "email_deliveries" ADD COLUMN "subject" text;--> statement-breakpoint
CREATE INDEX "idx_email_deliveries_created" ON "email_deliveries" USING btree ("created_at");