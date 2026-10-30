//! EXP-850 §3/§7: the `workflow` payload and the shared caption. EXP-1426:
//! the module lives in `exponential_agent_runtime::wire::workflow` now and is
//! re-exported here whole.

pub use exponential_agent_runtime::wire::workflow::*;

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn the_status_and_state_vocabularies_match_the_contract() {
        let statuses: Vec<&str> = WorkflowStatus::ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(statuses, domain::contract::WORKFLOW_STATUS_VALUES);
        let states: Vec<&str> = WorkflowAgentState::ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(states, domain::contract::WORKFLOW_AGENT_STATE_VALUES);
    }
}
