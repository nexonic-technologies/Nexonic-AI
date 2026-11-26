//! Private and collective memory for Sharee and Nexa.
//!
//! Three domains are kept apart by the storage API itself, not by prompts:
//!
//! * `Private(Sharee)` — readable only by Sharee (and the user).
//! * `Private(Nexa)` — readable only by Nexa (and the user).
//! * `Collective` — readable by both agents and the user.
//!
//! A record can also be marked confidential with an explicit list of agents it
//! must never reach ("remember this, but never share it with Sharee"). That rule
//! beats every sharing rule: such a record can't be promoted to collective
//! memory, can't be included in summaries for an excluded agent, and is removed
//! from every derived index when deleted.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The two independent personalities. Names are user-customizable in the app;
/// these identifiers are stable internal ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AgentId {
    Sharee,
    Nexa,
}

/// Who is reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    User,
    Agent(AgentId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Domain {
    Private(AgentId),
    Collective,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Episodic,
    Semantic,
    Procedural,
    Preference,
    Prospective,
    Reflective,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemoryId(u64);

#[derive(Debug, Clone, PartialEq)]
pub struct Memory {
    pub id: MemoryId,
    pub domain: Domain,
    pub kind: Kind,
    pub text: String,
    /// Where it came from (conversation id, file, observation…).
    pub source: String,
    /// 0.0–1.0.
    pub confidence: f32,
    /// Agents this record must never reach, whatever the domain.
    pub never_share_with: BTreeSet<AgentId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryError {
    NotFound,
    /// The reader may not see this record. Deliberately the same for
    /// "doesn't exist" and "not yours" so isolation leaks nothing.
    Forbidden,
    /// Confidential records can't move into collective memory.
    ConfidentialityConflict,
    InvalidConfidence,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MemoryError {}

/// Input for a new record.
#[derive(Debug, Clone)]
pub struct NewMemory {
    pub domain: Domain,
    pub kind: Kind,
    pub text: String,
    pub source: String,
    pub confidence: f32,
    pub never_share_with: BTreeSet<AgentId>,
}

/// In-memory store with the isolation rules. Encrypted SQLite persistence will
/// implement the same API (see docs/IMPLEMENTATION_STATUS.md).
#[derive(Debug, Default)]
pub struct MemoryStore {
    next: u64,
    records: BTreeMap<MemoryId, Memory>,
    /// Derived keyword index (stands in for vector/summary indexes): every
    /// derived structure must be updated on delete.
    index: BTreeMap<String, BTreeSet<MemoryId>>,
}

fn can_read(reader: Reader, m: &Memory) -> bool {
    match reader {
        Reader::User => true,
        Reader::Agent(a) => {
            if m.never_share_with.contains(&a) {
                return false;
            }
            match m.domain {
                Domain::Collective => true,
                Domain::Private(owner) => owner == a,
            }
        }
    }
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1)
        .map(|w| w.to_lowercase())
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn remember(&mut self, new: NewMemory) -> Result<MemoryId, MemoryError> {
        if !(0.0..=1.0).contains(&new.confidence) {
            return Err(MemoryError::InvalidConfidence);
        }
        if new.domain == Domain::Collective && !new.never_share_with.is_empty() {
            return Err(MemoryError::ConfidentialityConflict);
        }
        self.next += 1;
        let id = MemoryId(self.next);
        for w in words(&new.text) {
            self.index.entry(w).or_default().insert(id);
        }
        self.records.insert(
            id,
            Memory {
                id,
                domain: new.domain,
                kind: new.kind,
                text: new.text,
                source: new.source,
                confidence: new.confidence,
                never_share_with: new.never_share_with,
            },
        );
        Ok(id)
    }

    pub fn get(&self, reader: Reader, id: MemoryId) -> Result<&Memory, MemoryError> {
        match self.records.get(&id) {
            Some(m) if can_read(reader, m) => Ok(m),
            _ => Err(MemoryError::Forbidden),
        }
    }

    /// Keyword search, filtered by what `reader` may see.
    pub fn search(&self, reader: Reader, query: &str) -> Vec<&Memory> {
        let mut hits: BTreeSet<MemoryId> = BTreeSet::new();
        for w in words(query) {
            if let Some(ids) = self.index.get(&w) {
                hits.extend(ids.iter().copied());
            }
        }
        hits.iter()
            .filter_map(|id| self.records.get(id))
            .filter(|m| can_read(reader, m))
            .collect()
    }

    /// Text an agent may be given as context (e.g. a summary of what the other
    /// agent knows). Only records readable by `for_reader` are included.
    pub fn context_for(&self, for_reader: Reader) -> Vec<&str> {
        self.records
            .values()
            .filter(|m| can_read(for_reader, m))
            .map(|m| m.text.as_str())
            .collect()
    }

    /// Moves a private record into collective memory. Only its owner (or the
    /// user) may do this, and never for a confidential record.
    pub fn share(&mut self, by: Reader, id: MemoryId) -> Result<(), MemoryError> {
        let m = self.records.get_mut(&id).ok_or(MemoryError::Forbidden)?;
        let owner_ok = match (by, m.domain) {
            (Reader::User, _) => true,
            (Reader::Agent(a), Domain::Private(owner)) => a == owner,
            (Reader::Agent(_), Domain::Collective) => false,
        };
        if !owner_ok {
            return Err(MemoryError::Forbidden);
        }
        if !m.never_share_with.is_empty() {
            return Err(MemoryError::ConfidentialityConflict);
        }
        m.domain = Domain::Collective;
        Ok(())
    }

    /// Deletes a record and every derived index entry that points at it.
    pub fn forget(&mut self, by: Reader, id: MemoryId) -> Result<(), MemoryError> {
        let m = self.records.get(&id).ok_or(MemoryError::NotFound)?;
        if !can_read(by, m) {
            return Err(MemoryError::Forbidden);
        }
        self.records.remove(&id);
        self.index.retain(|_, ids| {
            ids.remove(&id);
            !ids.is_empty()
        });
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    #[cfg(test)]
    fn index_mentions(&self, id: MemoryId) -> bool {
        self.index.values().any(|ids| ids.contains(&id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHAREE: Reader = Reader::Agent(AgentId::Sharee);
    const NEXA: Reader = Reader::Agent(AgentId::Nexa);

    fn mem(domain: Domain, text: &str, never: &[AgentId]) -> NewMemory {
        NewMemory {
            domain,
            kind: Kind::Episodic,
            text: text.into(),
            source: "test".into(),
            confidence: 0.9,
            never_share_with: never.iter().copied().collect(),
        }
    }

    #[test]
    fn private_memory_is_invisible_to_the_other_agent() {
        let mut s = MemoryStore::new();
        let id = s
            .remember(mem(Domain::Private(AgentId::Nexa), "birthday plans", &[]))
            .unwrap();
        assert!(s.get(NEXA, id).is_ok());
        assert!(s.get(Reader::User, id).is_ok());
        assert_eq!(s.get(SHAREE, id), Err(MemoryError::Forbidden));
        assert!(s.search(SHAREE, "birthday").is_empty());
        assert!(s.context_for(SHAREE).is_empty());
    }

    #[test]
    fn collective_memory_is_visible_to_both() {
        let mut s = MemoryStore::new();
        let id = s
            .remember(mem(Domain::Collective, "user prefers dark theme", &[]))
            .unwrap();
        assert!(s.get(SHAREE, id).is_ok());
        assert!(s.get(NEXA, id).is_ok());
        assert_eq!(s.search(SHAREE, "dark").len(), 1);
    }

    #[test]
    fn never_share_with_cannot_be_promoted_or_leaked() {
        // "Nexa, remember this conversation, but never share it with Sharee."
        let mut s = MemoryStore::new();
        let id = s
            .remember(mem(
                Domain::Private(AgentId::Nexa),
                "salary negotiation",
                &[AgentId::Sharee],
            ))
            .unwrap();
        assert_eq!(s.share(NEXA, id), Err(MemoryError::ConfidentialityConflict));
        assert_eq!(
            s.share(Reader::User, id),
            Err(MemoryError::ConfidentialityConflict)
        );
        assert_eq!(s.get(SHAREE, id), Err(MemoryError::Forbidden));
        assert!(s.search(SHAREE, "salary").is_empty());
        assert!(s.context_for(SHAREE).is_empty());
        assert_eq!(s.context_for(NEXA), vec!["salary negotiation"]);
    }

    #[test]
    fn confidential_records_cannot_be_written_to_collective() {
        let mut s = MemoryStore::new();
        let r = s.remember(mem(Domain::Collective, "secret", &[AgentId::Sharee]));
        assert_eq!(r, Err(MemoryError::ConfidentialityConflict));
    }

    #[test]
    fn only_the_owner_can_share_a_private_record() {
        let mut s = MemoryStore::new();
        let id = s
            .remember(mem(Domain::Private(AgentId::Sharee), "rust tip", &[]))
            .unwrap();
        assert_eq!(s.share(NEXA, id), Err(MemoryError::Forbidden));
        s.share(SHAREE, id).unwrap();
        assert!(s.get(NEXA, id).is_ok());
    }

    #[test]
    fn forget_removes_record_and_derived_index_entries() {
        let mut s = MemoryStore::new();
        let id = s
            .remember(mem(Domain::Private(AgentId::Sharee), "old address", &[]))
            .unwrap();
        assert_eq!(s.forget(NEXA, id), Err(MemoryError::Forbidden));
        s.forget(Reader::User, id).unwrap();
        assert!(s.is_empty());
        assert!(!s.index_mentions(id));
        assert!(s.search(Reader::User, "address").is_empty());
    }

    #[test]
    fn confidence_must_be_in_range() {
        let mut s = MemoryStore::new();
        let mut m = mem(Domain::Collective, "x y", &[]);
        m.confidence = 1.5;
        assert_eq!(s.remember(m), Err(MemoryError::InvalidConfidence));
    }
}
