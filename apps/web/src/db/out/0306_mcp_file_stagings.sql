CREATE TABLE "mcp_file_stagings" (
	"attachment_id" uuid PRIMARY KEY NOT NULL,
	"user_id" text NOT NULL,
	"expires_at" timestamp with time zone NOT NULL
);
--> statement-breakpoint
CREATE INDEX "idx_mcp_file_stagings_expires" ON "mcp_file_stagings" USING btree ("expires_at");