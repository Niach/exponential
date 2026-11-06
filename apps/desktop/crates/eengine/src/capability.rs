//! VAPP-262: the capability runtime. A host's [`Capability`] is a native
//! tool provider (photos, files, share, …; E5) mounted under its catalog
//! name. ONE [`Broker`] per app instance answers app UI calls
//! (`photos.pick`) and agent calls (`photos__pick`, through the in-process
//! hub, [`crate::hub`]) the same way: the package's manifest is the ceiling,
//! the person's consent next ([`ConsentUi`], or the host paints the sheet
//! and answers through [`Broker::answer`]), then one audit row. Grants
//! persist in the Vault and change ONLY through [`Broker::set_grant`] with
//! `GrantVia::ConsentUi` after a gesture; no tool writes the package, the
//! grants or the host config.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::broker::{broker_audit_row, broker_decide, BrokerAuditRow, BrokerCall, CallFrom, CallOutcome, GrantValue, Grants, ManifestEntry, Verdict, VerdictKind};
use crate::consent::{consent_answer, consent_sheet, grant_write, grants_record, manage_sheet, ConsentAnswer, GrantVia};
use crate::providers::{Failure, Vault};

/// A native tool provider with the catalog's scopes (capabilities.json):
/// MCP tool objects in, MCP call results out.
pub trait Capability: Send + Sync {
    /// The catalog name it is mounted under (`photos`).
    fn name(&self) -> &str;
    /// Its tools as MCP tool objects (`name` = the catalog's tool, `inputSchema`, …).
    fn tools(&self) -> Vec<Value>;
    /// One call; `Ok` = an MCP call result (`content`, `isError?`).
    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, Failure>;
}

/// The ask sheet the person answers (consent.json `consentSheet`): a genUI surface.
#[derive(Clone, Debug, PartialEq)]
pub struct ConsentSheet {
    pub scope: String,
    pub components: Vec<Value>,
    pub data: Value,
}

/// A host that answers the sheet inline (it paints the surface and blocks
/// for the answer). Without one, [`Broker::call`] answers
/// [`BrokerAnswer::Ask`] and the host calls [`Broker::answer`] later.
pub trait ConsentUi: Send + Sync {
    fn ask(&self, app: &str, sheet: &ConsentSheet) -> ConsentAnswer;
}

/// Where the broker's audit rows go (audit.json's shape, the host's store).
pub trait CallAudit: Send + Sync {
    fn record(&self, row: BrokerAuditRow);
}

/// What a brokered call answers.
#[derive(Clone, Debug, PartialEq)]
pub enum BrokerAnswer {
    /// The capability's MCP call result.
    Result(Value),
    /// The person decides first: paint the sheet, then [`Broker::answer`].
    Ask(Verdict, ConsentSheet),
    /// Refused; `code` = `UNKNOWN_TOOL` for unknown, else `FORBIDDEN`; `message` = the reason.
    Refused(Failure),
}

/// One app instance's broker.
pub struct Broker {
    app_id: String,
    app_name: String,
    signer: String,
    manifest: Vec<ManifestEntry>,
    grants: Mutex<Grants>,
    foreground: AtomicBool,
    capabilities: Vec<Arc<dyn Capability>>,
    vault: Arc<dyn Vault>,
    consent: Option<Arc<dyn ConsentUi>>,
    audit: Option<Arc<dyn CallAudit>>,
}

/// What [`Broker::new`] takes.
pub struct BrokerSetup {
    pub app_id: String,
    pub app_name: String,
    pub signer: String,
    pub manifest: Vec<ManifestEntry>,
    pub grants: Grants,
    pub capabilities: Vec<Arc<dyn Capability>>,
    pub vault: Arc<dyn Vault>,
    pub consent: Option<Arc<dyn ConsentUi>>,
    pub audit: Option<Arc<dyn CallAudit>>,
}

impl Broker {
    pub fn new(setup: BrokerSetup) -> Self {
        Broker {
            app_id: setup.app_id,
            app_name: setup.app_name,
            signer: setup.signer,
            manifest: setup.manifest,
            grants: Mutex::new(setup.grants),
            foreground: AtomicBool::new(false),
            capabilities: setup.capabilities,
            vault: setup.vault,
            consent: setup.consent,
            audit: setup.audit,
        }
    }

    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    pub fn manifest(&self) -> &[ManifestEntry] {
        &self.manifest
    }

    pub fn grants(&self) -> Grants {
        self.grants.lock().unwrap().clone()
    }

    pub fn capabilities(&self) -> &[Arc<dyn Capability>] {
        &self.capabilities
    }

    /// The lifecycle tells the broker whether a sheet may show.
    pub fn set_foreground(&self, foreground: bool) {
        self.foreground.store(foreground, Ordering::SeqCst);
    }

    fn provided(&self) -> Vec<String> {
        self.capabilities.iter().map(|c| c.name().to_string()).collect()
    }

    fn record(&self, client: Option<&str>, call: &BrokerCall, verdict: &Verdict, outcome: Option<CallOutcome>) {
        if let Some(audit) = &self.audit {
            audit.record(broker_audit_row(&self.app_id, client, call, verdict, outcome));
        }
    }

    fn refused(verdict: &Verdict) -> BrokerAnswer {
        let code = if verdict.verdict == VerdictKind::Unknown { "UNKNOWN_TOOL" } else { "FORBIDDEN" };
        BrokerAnswer::Refused(Failure::new(code, verdict.reason.clone().unwrap_or_default()))
    }

    fn run(&self, client: Option<&str>, call: &BrokerCall, verdict: &Verdict) -> BrokerAnswer {
        let capability = self.capabilities.iter().find(|c| Some(c.name()) == verdict.capability.as_deref());
        let (answer, outcome) = match (capability, verdict.tool.as_deref()) {
            (Some(capability), Some(tool)) => match capability.call(tool, &call.arguments) {
                Ok(result) => {
                    let outcome = if result.get("isError").and_then(Value::as_bool) == Some(true) { CallOutcome::IsError } else { CallOutcome::Ok };
                    (BrokerAnswer::Result(result), outcome)
                }
                Err(failure) => (BrokerAnswer::Result(json!({"content": [{"type": "text", "text": failure.message}], "isError": true})), CallOutcome::RpcError),
            },
            _ => (BrokerAnswer::Refused(Failure::new("FORBIDDEN", "unavailable")), CallOutcome::Unreachable),
        };
        self.record(client, call, verdict, Some(outcome));
        answer
    }

    /// One call from app UI (`client` = None) or an agent run (`client` = its run id).
    pub fn call(&self, from: CallFrom, client: Option<&str>, name: &str, arguments: &Value) -> BrokerAnswer {
        let call = BrokerCall { from, name: name.to_string(), arguments: arguments.clone() };
        let grants = self.grants();
        let verdict = broker_decide(&self.manifest, &grants, &self.provided(), self.foreground.load(Ordering::SeqCst), &call);
        match verdict.verdict {
            VerdictKind::Call => self.run(client, &call, &verdict),
            VerdictKind::Ask => {
                let entry = self.manifest.iter().find(|m| Some(m.id.as_str()) == verdict.scope.as_deref()).cloned().unwrap_or_default();
                let (components, data) = consent_sheet(&self.app_name, &entry);
                let sheet = ConsentSheet { scope: entry.id.clone(), components, data };
                match &self.consent {
                    Some(ui) => {
                        let answer = ui.ask(&self.app_name, &sheet);
                        self.answer(from, client, name, arguments, &verdict, answer)
                    }
                    None => BrokerAnswer::Ask(verdict, sheet),
                }
            }
            _ => {
                self.record(client, &call, &verdict, None);
                Self::refused(&verdict)
            }
        }
    }

    /// The person's answer to the sheet for `verdict` (an `Ask`): store it
    /// per consent.json, then run or refuse the call. The answer is the gesture.
    pub fn answer(&self, from: CallFrom, client: Option<&str>, name: &str, arguments: &Value, verdict: &Verdict, answer: ConsentAnswer) -> BrokerAnswer {
        let call = BrokerCall { from, name: name.to_string(), arguments: arguments.clone() };
        let Some(scope) = verdict.scope.clone() else { return Self::refused(verdict) };
        let (kind, reason, next) = {
            let mut grants = self.grants.lock().unwrap();
            let (kind, reason, next) = consent_answer(&grants, &scope, answer);
            *grants = next.clone();
            (kind, reason, next)
        };
        self.persist(&next);
        let decided = Verdict { verdict: kind, reason: reason.map(str::to_string), ..verdict.clone() };
        if kind == VerdictKind::Call {
            self.run(client, &call, &decided)
        } else {
            self.record(client, &call, &decided, None);
            Self::refused(&decided)
        }
    }

    /// Change a grant (the Permissions sheet's Switch). Only the ConsentUi
    /// after a gesture; `value` None = revoke. `Err` = consent.json's refusal.
    pub fn set_grant(&self, via: GrantVia, gesture: bool, scope: &str, value: Option<GrantValue>) -> Result<(), &'static str> {
        let next = {
            let mut grants = self.grants.lock().unwrap();
            let (next, refused) = grant_write(&self.manifest, &grants, via, gesture, scope, value);
            if let Some(refused) = refused {
                return Err(refused);
            }
            *grants = next.clone();
            next
        };
        self.persist(&next);
        Ok(())
    }

    /// The Permissions sheet (consent.json `manageSheet`).
    pub fn permissions_sheet(&self) -> Vec<Value> {
        manage_sheet(&self.manifest, &self.grants())
    }

    fn persist(&self, grants: &Grants) {
        let (key, value) = grants_record(&self.app_id, &self.signer, &self.manifest, grants);
        // A failed write only means the person is asked again next launch.
        let _ = self.vault.write(&key, &value);
    }
}
