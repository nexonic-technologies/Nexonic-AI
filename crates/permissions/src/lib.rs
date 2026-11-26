//! Revocable capability grants for Nexonic AI.
//!
//! Every sensitive capability is off until the user grants it. Grants can be
//! scoped (for example to a set of folders), revoked one at a time, or all at
//! once with the emergency stop. Every change is recorded in an audit trail.
//! Agents can ask for a capability but can never grant one to themselves.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A capability the user can grant or revoke independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    Camera,
    Microphone,
    Screen,
    Applications,
    WebsiteContext,
    FilesAndFolders,
    ComputerAutomation,
    BackgroundReasoning,
    ProactiveVoice,
    AutomaticMemoryLearning,
    AutomaticSkillAcquisition,
    InterAgentCommunication,
    CloudContextSharing,
    RawRecordingRetention,
}

impl Capability {
    pub const ALL: [Capability; 14] = [
        Capability::Camera,
        Capability::Microphone,
        Capability::Screen,
        Capability::Applications,
        Capability::WebsiteContext,
        Capability::FilesAndFolders,
        Capability::ComputerAutomation,
        Capability::BackgroundReasoning,
        Capability::ProactiveVoice,
        Capability::AutomaticMemoryLearning,
        Capability::AutomaticSkillAcquisition,
        Capability::InterAgentCommunication,
        Capability::CloudContextSharing,
        Capability::RawRecordingRetention,
    ];
}

/// Operations that always need a fresh confirmation, even in Full Assistance Mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighImpactAction {
    FinancialTransaction,
    SensitiveExternalMessage,
    PermanentDeletion,
    SecuritySettingChange,
}

/// Who is acting. Only the user can change grants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    User,
    Agent(String),
    System,
}

/// Limits a grant to particular resources. `Any` means the whole capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Any,
    /// Path prefixes (for files) or bundle identifiers (for applications).
    Only(BTreeSet<String>),
}

impl Scope {
    fn allows(&self, resource: Option<&str>) -> bool {
        match (self, resource) {
            (Scope::Any, _) => true,
            (Scope::Only(_), None) => false,
            (Scope::Only(set), Some(r)) => set
                .iter()
                .any(|p| r == p || r.starts_with(&format!("{}/", p.trim_end_matches('/')))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    Denied(DenyReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    NotGranted,
    OutOfScope,
    EmergencyStopActive,
    NeedsConfirmation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionError {
    /// Only the user may grant, revoke or clear the emergency stop.
    NotAuthorized(Actor),
}

impl fmt::Display for PermissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PermissionError::NotAuthorized(a) => write!(f, "{a:?} may not change permissions"),
        }
    }
}

impl std::error::Error for PermissionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    Granted(Capability),
    Revoked(Capability),
    EmergencyStop,
    Resumed,
    Denied {
        actor: Actor,
        capability: Capability,
        reason: DenyReason,
    },
    RejectedChange {
        actor: Actor,
    },
}

/// The live permission state. Starts with nothing granted.
#[derive(Debug, Default)]
pub struct Permissions {
    grants: BTreeMap<Capability, Scope>,
    stopped: bool,
    audit: Vec<AuditEvent>,
}

impl Permissions {
    pub fn new() -> Self {
        Self::default()
    }

    fn require_user(&mut self, actor: &Actor) -> Result<(), PermissionError> {
        if *actor == Actor::User {
            Ok(())
        } else {
            self.audit.push(AuditEvent::RejectedChange {
                actor: actor.clone(),
            });
            Err(PermissionError::NotAuthorized(actor.clone()))
        }
    }

    pub fn grant(
        &mut self,
        by: &Actor,
        cap: Capability,
        scope: Scope,
    ) -> Result<(), PermissionError> {
        self.require_user(by)?;
        self.grants.insert(cap, scope);
        self.audit.push(AuditEvent::Granted(cap));
        Ok(())
    }

    pub fn revoke(&mut self, by: &Actor, cap: Capability) -> Result<(), PermissionError> {
        self.require_user(by)?;
        if self.grants.remove(&cap).is_some() {
            self.audit.push(AuditEvent::Revoked(cap));
        }
        Ok(())
    }

    /// Revokes every grant immediately. Anyone may trigger it, including an agent
    /// that detects a problem; only the user can resume.
    pub fn emergency_stop(&mut self) {
        self.grants.clear();
        self.stopped = true;
        self.audit.push(AuditEvent::EmergencyStop);
    }

    /// Lifts the stop. Grants stay revoked until the user grants them again.
    pub fn resume(&mut self, by: &Actor) -> Result<(), PermissionError> {
        self.require_user(by)?;
        self.stopped = false;
        self.audit.push(AuditEvent::Resumed);
        Ok(())
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Checks whether `actor` may use `cap` on `resource`. Denials are audited.
    pub fn check(&mut self, actor: &Actor, cap: Capability, resource: Option<&str>) -> Decision {
        let decision = if self.stopped {
            Decision::Denied(DenyReason::EmergencyStopActive)
        } else {
            match self.grants.get(&cap) {
                None => Decision::Denied(DenyReason::NotGranted),
                Some(scope) if !scope.allows(resource) => Decision::Denied(DenyReason::OutOfScope),
                Some(_) => Decision::Allowed,
            }
        };
        if let Decision::Denied(reason) = &decision {
            self.audit.push(AuditEvent::Denied {
                actor: actor.clone(),
                capability: cap,
                reason: reason.clone(),
            });
        }
        decision
    }

    /// High-impact actions are never pre-authorized: the caller must pass a
    /// confirmation the user gave for this specific action.
    pub fn check_high_impact(
        &mut self,
        _action: HighImpactAction,
        user_confirmed: bool,
    ) -> Decision {
        if self.stopped {
            Decision::Denied(DenyReason::EmergencyStopActive)
        } else if user_confirmed {
            Decision::Allowed
        } else {
            Decision::Denied(DenyReason::NeedsConfirmation)
        }
    }

    pub fn granted(&self) -> impl Iterator<Item = (&Capability, &Scope)> {
        self.grants.iter()
    }

    pub fn audit(&self) -> &[AuditEvent] {
        &self.audit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> Actor {
        Actor::Agent("sharee".into())
    }

    #[test]
    fn everything_is_off_by_default() {
        let mut p = Permissions::new();
        for cap in Capability::ALL {
            assert_eq!(
                p.check(&agent(), cap, None),
                Decision::Denied(DenyReason::NotGranted)
            );
        }
    }

    #[test]
    fn agents_cannot_grant_themselves() {
        let mut p = Permissions::new();
        assert!(p.grant(&agent(), Capability::Camera, Scope::Any).is_err());
        assert!(p.resume(&agent()).is_err());
        assert_eq!(
            p.check(&agent(), Capability::Camera, None),
            Decision::Denied(DenyReason::NotGranted)
        );
        assert!(matches!(p.audit()[0], AuditEvent::RejectedChange { .. }));
    }

    #[test]
    fn grant_and_revoke() {
        let mut p = Permissions::new();
        p.grant(&Actor::User, Capability::Microphone, Scope::Any)
            .unwrap();
        assert_eq!(
            p.check(&agent(), Capability::Microphone, None),
            Decision::Allowed
        );
        p.revoke(&Actor::User, Capability::Microphone).unwrap();
        assert_eq!(
            p.check(&agent(), Capability::Microphone, None),
            Decision::Denied(DenyReason::NotGranted)
        );
    }

    #[test]
    fn folder_scope_is_enforced() {
        let mut p = Permissions::new();
        let scope = Scope::Only(["/Users/me/Documents".to_string()].into());
        p.grant(&Actor::User, Capability::FilesAndFolders, scope)
            .unwrap();
        let cap = Capability::FilesAndFolders;
        assert_eq!(
            p.check(&agent(), cap, Some("/Users/me/Documents/a.txt")),
            Decision::Allowed
        );
        assert_eq!(
            p.check(&agent(), cap, Some("/Users/me/Documents")),
            Decision::Allowed
        );
        assert_eq!(
            p.check(&agent(), cap, Some("/Users/me/DocumentsSecret/x")),
            Decision::Denied(DenyReason::OutOfScope)
        );
        assert_eq!(
            p.check(&agent(), cap, Some("/Users/me/.ssh/id")),
            Decision::Denied(DenyReason::OutOfScope)
        );
        assert_eq!(
            p.check(&agent(), cap, None),
            Decision::Denied(DenyReason::OutOfScope)
        );
    }

    #[test]
    fn emergency_stop_revokes_everything_until_user_regrants() {
        let mut p = Permissions::new();
        for cap in Capability::ALL {
            p.grant(&Actor::User, cap, Scope::Any).unwrap();
        }
        p.emergency_stop();
        assert!(p.is_stopped());
        assert_eq!(p.granted().count(), 0);
        assert_eq!(
            p.check(&agent(), Capability::Screen, None),
            Decision::Denied(DenyReason::EmergencyStopActive)
        );
        p.resume(&Actor::User).unwrap();
        assert_eq!(
            p.check(&agent(), Capability::Screen, None),
            Decision::Denied(DenyReason::NotGranted)
        );
    }

    #[test]
    fn high_impact_actions_always_need_confirmation() {
        let mut p = Permissions::new();
        for cap in Capability::ALL {
            p.grant(&Actor::User, cap, Scope::Any).unwrap();
        }
        let a = HighImpactAction::PermanentDeletion;
        assert_eq!(
            p.check_high_impact(a, false),
            Decision::Denied(DenyReason::NeedsConfirmation)
        );
        assert_eq!(p.check_high_impact(a, true), Decision::Allowed);
        p.emergency_stop();
        assert_eq!(
            p.check_high_impact(a, true),
            Decision::Denied(DenyReason::EmergencyStopActive)
        );
    }
}
