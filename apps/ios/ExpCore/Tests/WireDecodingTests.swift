import XCTest
@testable import ExpCore

// Locks the type-aware wire decoding (WireDecoding.swift + the per-entity
// decoders). Electric's shape wire format delivers every column as a JSON
// string, so these fixtures feed string-shaped JSON straight into the real
// entities and assert numeric/boolean fields parse while String fields keep
// their verbatim (numeric/boolean-looking) text. Also covers the native-JSON
// (tRPC/fixture) scalar form and the helper edge cases.
final class WireDecodingTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder().decode(T.self, from: Data(json.utf8))
    }

    // MARK: - Issue

    func testIssueDecodesWireStringNumbersAndKeepsNumericTitle() throws {
        let issue = try decode(IssueEntity.self, #"""
        {
          "id": "i1", "board_id": "p1", "title": "404",
          "status": "todo", "priority": "none",
          "number": "7", "pr_number": "12", "sort_order": "3.5",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertEqual(issue.number, 7)
        XCTAssertEqual(issue.prNumber, 12)
        XCTAssertEqual(issue.sortOrder, 3.5)
        XCTAssertEqual(issue.title, "404")
    }

    func testIssueDecodesNativeJSONNumbers() throws {
        // The tRPC/fixture form uses native JSON scalars — still valid.
        let issue = try decode(IssueEntity.self, #"""
        {
          "id": "i1", "board_id": "p1", "title": "Real",
          "status": "todo", "priority": "none",
          "number": 7, "sort_order": 3.5,
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertEqual(issue.number, 7)
        XCTAssertEqual(issue.sortOrder, 3.5)
        XCTAssertNil(issue.prNumber)
    }

    // MARK: - Label

    func testLabelDecodesWireSortOrderAndKeepsBooleanLookingName() throws {
        let label = try decode(LabelEntity.self, #"""
        {
          "id": "l1", "team_id": "w1", "name": "true", "color": "#fff",
          "sort_order": "1.5",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertEqual(label.sortOrder, 1.5)
        XCTAssertEqual(label.name, "true")
    }

    // MARK: - Team (helpdesk_enabled rides the teams shape as Postgres text)

    func testTeamDecodesWireHelpdeskEnabled() throws {
        let team = try decode(TeamEntity.self, #"""
        {
          "id": "w1", "name": "Team", "slug": "team", "helpdesk_enabled": "t",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertTrue(team.helpdeskEnabled)
    }

    func testTeamHelpdeskEnabledDefaultsFalseWhenAbsent() throws {
        // A pre-rotation snapshot may omit the column — schema default wins.
        let team = try decode(TeamEntity.self, #"""
        {
          "id": "w1", "name": "Team", "slug": "team",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertFalse(team.helpdeskEnabled)
    }

    // MARK: - Board (sort_order arrives as Postgres text off the wire)

    func testBoardDecodesWireSortOrderAndIgnoresDeadColumns() throws {
        // `is_protected` was dropped server-side (EXP-364) — a stale snapshot
        // that still carries it must decode fine (Codable drops unknown keys).
        let board = try decode(BoardEntity.self, #"""
        {
          "id": "p1", "team_id": "w1", "name": "P", "slug": "p", "prefix": "P",
          "sort_order": "2", "is_protected": "t",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertEqual(board.sortOrder, 2)
    }

    // MARK: - Attachment

    func testAttachmentDecodesWireIntsAndKeepsNumericFilename() throws {
        let attachment = try decode(AttachmentEntity.self, #"""
        {
          "id": "a1", "team_id": "w1", "issue_id": "i1", "uploader_id": "u1",
          "filename": "3.5", "content_type": "image/png",
          "size_bytes": "12345", "storage_key": "k", "url": "/x",
          "width": "800", "height": "600",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertEqual(attachment.sizeBytes, 12345)
        XCTAssertEqual(attachment.width, 800)
        XCTAssertEqual(attachment.height, 600)
        XCTAssertEqual(attachment.filename, "3.5")
        XCTAssertEqual(attachment.uploaderId, "u1")
    }

    // REV-7: uploader_id is NULLABLE server-side — a widget screenshot has no
    // human uploader, and the FK is ON DELETE SET NULL, so a deleted account
    // nulls the uploader on attachments left behind in a surviving team. A
    // strict decode here dropped those inserts on the floor forever.
    func testAttachmentDecodesNullUploader() throws {
        let attachment = try decode(AttachmentEntity.self, #"""
        {
          "id": "a1", "team_id": "w1", "issue_id": "i1", "uploader_id": null,
          "filename": "screenshot.png", "content_type": "image/png",
          "size_bytes": "12345", "storage_key": "k", "url": "/x",
          "width": null, "height": null,
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertNil(attachment.uploaderId)
        XCTAssertEqual(attachment.filename, "screenshot.png")
    }

    // MARK: - Notification (issue-less support_reply rows carry team_id)

    func testNotificationDecodesIssuelessSupportReplyWithTeamId() throws {
        let notification = try decode(NotificationEntity.self, #"""
        {
          "id": "n1", "user_id": "u1", "issue_id": null, "team_id": "w1",
          "type": "support_reply", "title": "New reply on ticket",
          "body": "A customer replied",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertNil(notification.issueId)
        XCTAssertEqual(notification.teamId, "w1")
        XCTAssertEqual(notification.type, "support_reply")
    }

    func testNotificationTeamIdAbsentOrNullIsNil() throws {
        // Issue-anchored rows carry team_id: null; pre-rotation snapshots may
        // omit the key entirely — both must decode with a nil teamId.
        let notification = try decode(NotificationEntity.self, #"""
        {
          "id": "n2", "user_id": "u1", "issue_id": "i1",
          "type": "issue_comment", "title": "New comment",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertEqual(notification.issueId, "i1")
        XCTAssertNil(notification.teamId)
    }

    // MARK: - IssueSubscriber (bare Postgres "t")

    func testIssueSubscriberDecodesWirePostgresTrue() throws {
        let sub = try decode(IssueSubscriberEntity.self, #"""
        {
          "id": "s1", "issue_id": "i1", "team_id": "w1", "source": "manual",
          "unsubscribed": "t",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertTrue(sub.unsubscribed)
    }

    // MARK: - Action (inputs jsonb rides the actions shape, EXP-268)

    func testActionDecodesObjectArrayInputsToStoredString() throws {
        // jsonb arrives as a real JSON array off the wire — it must re-encode
        // to a stored string TYPE-FAITHFULLY (`required` stays a boolean) so
        // the later ActionInputDto parse round-trips.
        let action = try decode(ActionEntity.self, #"""
        {
          "id": "a1", "team_id": "w1", "repository_id": "r1",
          "name": "Deploy", "description": "Ship it", "sort_order": "2.5",
          "inputs": [
            {"key": "env", "label": "Environment", "type": "text",
             "required": true, "placeholder": "prod"},
            {"key": "repo", "label": "Repository", "type": "repo"}
          ],
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertEqual(action.sortOrder, 2.5)
        XCTAssertEqual(action.repositoryId, "r1")
        let stored = try XCTUnwrap(action.inputs)
        let parsed = try JSONDecoder().decode([ActionInputDto].self, from: Data(stored.utf8))
        XCTAssertEqual(parsed, [
            ActionInputDto(
                key: "env", label: "Environment", type: "text",
                required: true, placeholder: "prod"
            ),
            ActionInputDto(key: "repo", label: "Repository", type: "repo"),
        ])
    }

    func testActionDecodesStringInputsVerbatimAndNullAsNil() throws {
        // A string-typed jsonb wire form stores verbatim; null / absent → nil.
        let stringForm = try decode(ActionEntity.self, #"""
        {
          "id": "a1", "team_id": "w1", "name": "Deploy",
          "inputs": "[{\"key\":\"env\",\"label\":\"Env\",\"type\":\"text\"}]",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertEqual(stringForm.inputs, #"[{"key":"env","label":"Env","type":"text"}]"#)

        let nullForm = try decode(ActionEntity.self, #"""
        {
          "id": "a2", "team_id": "w1", "name": "Sweep", "inputs": null,
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertNil(nullForm.inputs)

        let absentForm = try decode(ActionEntity.self, #"""
        {
          "id": "a3", "team_id": "w1", "name": "Report",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertNil(absentForm.inputs)
    }

    // MARK: - AuthConfig (/api/auth-config)

    func testAuthConfigDecodesSignupAndResetFlags() throws {
        let config = try decode(AuthConfig.self, #"""
        {
          "passwordEnabled": true,
          "signupEnabled": true,
          "passwordResetEnabled": true,
          "oidcProviders": [{ "id": "authentik", "name": "Authentik" }],
          "googleLoginEnabled": false,
          "appleLoginEnabled": false,
          "githubEnabled": true
        }
        """#)
        XCTAssertTrue(config.signupEnabled)
        XCTAssertTrue(config.passwordResetEnabled)
        XCTAssertEqual(config.oidcProviders.first?.id, "authentik")
    }

    func testAuthConfigMissingFlagsDefaultToFalse() throws {
        // An older self-hosted server may not publish the flags — the login
        // screen must hide the hand-off rather than offer a dead link.
        let config = try decode(AuthConfig.self, #"""
        {
          "passwordEnabled": true,
          "oidcProviders": [],
          "googleLoginEnabled": false
        }
        """#)
        XCTAssertFalse(config.signupEnabled)
        XCTAssertFalse(config.passwordResetEnabled)
        XCTAssertFalse(config.appleLoginEnabled)
    }

    func testAuthWebHandoffUrlsTrimTrailingSlash() {
        XCTAssertEqual(
            AuthApi.registerUrl(instanceUrl: "https://app.exponential.at/")?.absoluteString,
            "https://app.exponential.at/auth/register"
        )
        XCTAssertEqual(
            AuthApi.forgotPasswordUrl(instanceUrl: "https://app.exponential.at")?.absoluteString,
            "https://app.exponential.at/auth/forgot-password"
        )
        XCTAssertNil(AuthApi.registerUrl(instanceUrl: "  "))
        XCTAssertNil(AuthApi.forgotPasswordUrl(instanceUrl: nil))
    }

    // MARK: - Helper edges

    func testWireIntThrowsOnUnparseableString() {
        // Garbage in a numeric field must surface as a decode drop, not silently
        // become nil.
        XCTAssertThrowsError(try decode(IssueEntity.self, #"""
        {
          "id": "i1", "board_id": "p1", "title": "t",
          "status": "todo", "priority": "none", "number": "not-a-number",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#))
    }

    func testWireIntAbsentOrNullIsNil() throws {
        let issue = try decode(IssueEntity.self, #"""
        {
          "id": "i1", "board_id": "p1", "title": "t",
          "status": "todo", "priority": "none", "number": null,
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertNil(issue.number)   // explicit JSON null
        XCTAssertNil(issue.prNumber) // absent key
    }

    func testWireBoolUnknownStringFallsBackToDefault() throws {
        // The wire-bool default (here `unsubscribed` → false); an unrecognized
        // string keeps the default.
        let sub = try decode(IssueSubscriberEntity.self, #"""
        {
          "id": "s1", "issue_id": "i1", "team_id": "w1", "source": "manual",
          "unsubscribed": "maybe",
          "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        }
        """#)
        XCTAssertFalse(sub.unsubscribed)
    }
}
