import XCTest

@testable import ExpCore

// EXP-724 — the steer slash-command catalog and its two matching rules.
// Mirrors the web `steer-commands.test.ts`, Android's `SlashCommandsTest` and
// desktop's `slash_commands.rs` tests: same catalog lock, same query rules,
// same insertion and send rules. NEW-2 (owner 2026-10-21): the catalog = the
// contract `steerCommands` (compact, model, effort; `clear` and `btw` gone)
// and the commands an agent CLI injects (`config_state.commands`) are HIDDEN:
// nothing merges them into the menu or the command lookup.
final class SlashCommandsTests: XCTestCase {

    // MARK: - Catalog lock

    func testCatalogZipsEveryContractRow() {
        XCTAssertEqual(SlashCommands.all.count, DomainContract.steerCommandNames.count)
        XCTAssertEqual(SlashCommands.all.map(\.name), DomainContract.steerCommandNames)
        XCTAssertEqual(
            SlashCommands.all.map(\.description), DomainContract.steerCommandDescriptions
        )
        XCTAssertEqual(SlashCommands.all.map(\.argHint), DomainContract.steerCommandArgHints)
        XCTAssertEqual(SlashCommands.all.map(\.confirm), DomainContract.steerCommandConfirm)
    }

    func testTheCatalogIsCompactModelEffort() {
        XCTAssertEqual(SlashCommands.all.map(\.name), ["compact", "model", "effort"])
        XCTAssertNil(SlashCommands.all.first { $0.name == "clear" })
        XCTAssertNil(SlashCommands.all.first { $0.name == "btw" })
    }

    func testEveryCommandRunsOnKnownAgentsOnly() {
        let known = Set(DomainContract.codingAgentValues)
        for command in SlashCommands.all {
            XCTAssertFalse(command.agents.isEmpty, "\(command.name) runs nowhere")
            for agent in command.agents {
                XCTAssertTrue(known.contains(agent), "\(command.name) names unknown agent \(agent)")
            }
        }
    }

    func testNamesAreUniqueAndTokenShaped() {
        let names = SlashCommands.all.map(\.name)
        XCTAssertEqual(Set(names).count, names.count)
        for name in names {
            XCTAssertFalse(name.isEmpty)
            XCTAssertFalse(name.contains(where: { $0.isWhitespace }), "\(name) carries whitespace")
            XCTAssertFalse(name.hasPrefix("/"), "\(name) carries its own slash")
        }
    }

    func testDefaultAgentIsTheFirstContractAgent() {
        XCTAssertEqual(SlashCommands.defaultAgent, DomainContract.codingAgentValues[0])
        XCTAssertEqual(SlashCommands.defaultAgent, "claude")
    }

    // MARK: - Per-agent catalog

    func testCatalogIsFilteredByAgentInContractOrder() {
        // The same three rows for every CONTRACT agent; an id outside the
        // contract (EXP-849: a retired agent, an external ACP binary) gets no
        // curated rows.
        for agent in DomainContract.codingAgentValues {
            XCTAssertEqual(SlashCommands.catalog(for: agent).map(\.name), ["compact", "model", "effort"])
        }
        XCTAssertTrue(SlashCommands.catalog(for: "zed").isEmpty)
    }

    func testAnAbsentAgentFallsBackToClaude() {
        XCTAssertEqual(
            SlashCommands.catalog(for: nil).map(\.name),
            SlashCommands.catalog(for: "claude").map(\.name)
        )
        XCTAssertEqual(
            SlashCommands.catalog(for: "").map(\.name),
            SlashCommands.catalog(for: "claude").map(\.name)
        )
    }

    func testAnUnknownAgentRunsNothing() {
        XCTAssertTrue(SlashCommands.catalog(for: "gemini").isEmpty)
    }

    /// EXP-746: "no agent" alone does not mean claude — an EXTERNAL ACP agent
    /// syncs no agent either, and its curated catalog is empty. NEW-2: what it
    /// advertises stays hidden too, so its menu is empty.
    func testAnAgentLessAcpRunIsAnExternalAgent() {
        XCTAssertFalse(DomainContract.codingAgentValues.contains(SlashCommands.externalAgent))
        XCTAssertEqual(SlashCommands.agentId(nil, acp: true), SlashCommands.externalAgent)
        XCTAssertEqual(SlashCommands.agentId("  ", acp: true), SlashCommands.externalAgent)
        XCTAssertEqual(SlashCommands.agentId("codex", acp: true), "codex")
        XCTAssertEqual(SlashCommands.agentId(nil, acp: false), SlashCommands.defaultAgent)
        XCTAssertEqual(SlashCommands.agentId("", acp: false), SlashCommands.defaultAgent)

        let external = SlashCommands.agentId(nil, acp: true)
        XCTAssertTrue(SlashCommands.catalog(for: external).isEmpty)
        XCTAssertTrue(SlashCommands.matches(draft: "/", agent: external).isEmpty)
        XCTAssertNil(SlashCommands.command(for: "/compact", agent: external))
    }

    // MARK: - Menu query rule

    func testABareSlashListsEverythingTheAgentCanRun() {
        XCTAssertEqual(
            SlashCommands.matches(draft: "/", agent: "claude").map(\.name),
            ["compact", "model", "effort"]
        )
    }

    func testPrefixFilteringIsCaseInsensitive() {
        XCTAssertEqual(SlashCommands.matches(draft: "/co", agent: "claude").map(\.name), ["compact"])
        XCTAssertEqual(SlashCommands.matches(draft: "/CO", agent: "claude").map(\.name), ["compact"])
        XCTAssertEqual(SlashCommands.matches(draft: "/m", agent: "codex").map(\.name), ["model"])
        XCTAssertEqual(SlashCommands.matches(draft: "/E", agent: "claude").map(\.name), ["effort"])
    }

    func testTheMenuClosesOnceAnArgumentStarts() {
        XCTAssertTrue(SlashCommands.matches(draft: "/compact ", agent: "claude").isEmpty)
        XCTAssertTrue(SlashCommands.matches(draft: "/model opus", agent: "claude").isEmpty)
    }

    func testTheMenuNeedsTheSlashAtPositionZero() {
        XCTAssertTrue(SlashCommands.matches(draft: "x /c", agent: "claude").isEmpty)
        XCTAssertTrue(SlashCommands.matches(draft: " /c", agent: "claude").isEmpty)
        XCTAssertTrue(SlashCommands.matches(draft: "", agent: "claude").isEmpty)
        XCTAssertTrue(SlashCommands.matches(draft: "hello", agent: "claude").isEmpty)
    }

    func testANameThatMatchesNothingClosesTheMenu() {
        XCTAssertTrue(SlashCommands.matches(draft: "/zzz", agent: "claude").isEmpty)
        // The retired rows match nothing any more.
        XCTAssertTrue(SlashCommands.matches(draft: "/cl", agent: "claude").isEmpty)
        XCTAssertTrue(SlashCommands.matches(draft: "/btw", agent: "claude").isEmpty)
        // Non-name characters are not a partial name at all.
        XCTAssertTrue(SlashCommands.matches(draft: "/co/", agent: "claude").isEmpty)
    }

    // MARK: - Insertion

    func testInsertionLeavesRoomForTheArgument() {
        for command in SlashCommands.all {
            XCTAssertFalse(command.argHint.isEmpty, command.name)
            XCTAssertEqual(command.insertion, "/\(command.name) ")
        }
        XCTAssertEqual(SlashCommands.all.first { $0.name == "compact" }?.argHint, "instructions")
    }

    // MARK: - Send rule

    func testAMessageIsACommandOnlyWhenItsFirstTokenMatchesExactly() {
        XCTAssertEqual(SlashCommands.command(for: "/compact", agent: "claude")?.name, "compact")
        XCTAssertEqual(
            SlashCommands.command(for: "/compact keep the plan", agent: "claude")?.name, "compact"
        )
        XCTAssertEqual(SlashCommands.command(for: "/COMPACT", agent: "claude")?.name, "compact")
        XCTAssertEqual(SlashCommands.command(for: "/model opus", agent: "claude")?.name, "model")
        XCTAssertEqual(SlashCommands.command(for: "/effort high", agent: "codex")?.name, "effort")
        XCTAssertNil(SlashCommands.command(for: "/compactify", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "please /compact", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "//compact", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "compact", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "   ", agent: "claude"))
    }

    func testTheSendRuleIsAgentScopedToo() {
        XCTAssertNil(SlashCommands.command(for: "/compact", agent: "zed"))
        // Outside the catalog, whatever the CLI itself ships — the retired
        // rows and any skill / plugin command are prose.
        XCTAssertNil(SlashCommands.command(for: "/clear", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "/btw what now", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "/new", agent: "codex"))
        XCTAssertNil(SlashCommands.command(for: "/init", agent: "codex"))
        XCTAssertNil(SlashCommands.command(for: "/review", agent: "claude"))
    }

    // MARK: - Confirm

    func testNothingConfirmsAnyMore() {
        XCTAssertTrue(SlashCommands.all.filter(\.confirm).isEmpty)
    }

    /// Byte-identical ×4 (EXP-724): the copy stays for a future confirming
    /// contract row.
    func testConfirmCopyIsTheLockedLiterals() {
        guard let compact = SlashCommands.all.first(where: { $0.name == "compact" }) else {
            return XCTFail("no /compact row")
        }
        XCTAssertEqual(SlashCommands.confirmTitle(compact), "Run /compact?")
        XCTAssertEqual(SlashCommands.confirmButton(compact), "Run /compact")
        XCTAssertEqual(
            SlashCommands.confirmBody,
            "The agent forgets everything in this session so far. Files in the worktree are kept."
        )
    }
}
