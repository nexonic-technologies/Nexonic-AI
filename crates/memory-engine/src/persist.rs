//! Encrypted on-disk memory.
//!
//! Records live in SQLite. Text, source and stored values are sealed with
//! XChaCha20-Poly1305 under a 256-bit key that the app keeps in the macOS
//! Keychain; each ciphertext is bound to its row and field (associated data),
//! so ciphertexts can't be swapped between rows or fields. Isolation rules are
//! the same as [`MemoryStore`]: this type writes through and then delegates
//! every read to it.

use std::collections::BTreeSet;
use std::path::Path;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use zeroize::Zeroizing;

use crate::{AgentId, Domain, Kind, Memory, MemoryError, MemoryId, MemoryStore, NewMemory, Reader};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS memories (
  id INTEGER PRIMARY KEY,
  domain TEXT NOT NULL,
  kind TEXT NOT NULL,
  confidence REAL NOT NULL,
  never_share TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  text BLOB NOT NULL,
  source BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS kv (k TEXT PRIMARY KEY, v BLOB NOT NULL);
";

/// A fresh random 256-bit key (store it in the Keychain).
pub fn generate_key() -> Zeroizing<[u8; 32]> {
    let mut k = Zeroizing::new([0u8; 32]);
    rand::rng().fill_bytes(&mut k[..]);
    k
}

fn err<E: std::fmt::Display>(e: E) -> MemoryError {
    MemoryError::Storage(e.to_string())
}

pub struct PersistentMemory {
    db: Connection,
    cipher: XChaCha20Poly1305,
    store: MemoryStore,
}

impl PersistentMemory {
    /// Opens (or creates) the database at `path` and loads it with `key`.
    /// A wrong key is detected on open and refused.
    pub fn open(path: &Path, key: &[u8; 32]) -> Result<Self, MemoryError> {
        let db = Connection::open(path).map_err(err)?;
        Self::init(db, key)
    }

    /// In-memory database, for tests.
    pub fn open_in_memory(key: &[u8; 32]) -> Result<Self, MemoryError> {
        Self::init(Connection::open_in_memory().map_err(err)?, key)
    }

    fn init(db: Connection, key: &[u8; 32]) -> Result<Self, MemoryError> {
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA secure_delete=ON;",
        )
        .map_err(err)?;
        db.execute_batch(SCHEMA).map_err(err)?;
        let cipher = XChaCha20Poly1305::new(key.into());
        let mut me = Self {
            db,
            cipher,
            store: MemoryStore::new(),
        };
        me.check_key()?;
        me.load()?;
        Ok(me)
    }

    /// A sealed marker proves the key matches the database.
    fn check_key(&mut self) -> Result<(), MemoryError> {
        let marker: Option<Vec<u8>> = self
            .db
            .query_row("SELECT v FROM meta WHERE k = 'keycheck'", [], |r| r.get(0))
            .optional()
            .map_err(err)?;
        match marker {
            Some(blob) => {
                let plain = self.open_blob(&blob, b"meta:keycheck").map_err(|_| {
                    MemoryError::Storage("wrong key for this memory database".into())
                })?;
                if plain != b"nexonic-memory-v1" {
                    return Err(MemoryError::Storage(
                        "memory database marker mismatch".into(),
                    ));
                }
            }
            None => {
                let blob = self.seal(b"nexonic-memory-v1", b"meta:keycheck")?;
                self.db
                    .execute(
                        "INSERT INTO meta (k, v) VALUES ('keycheck', ?1)",
                        params![blob],
                    )
                    .map_err(err)?;
            }
        }
        Ok(())
    }

    fn seal(&self, plain: &[u8], aad: &[u8]) -> Result<Vec<u8>, MemoryError> {
        let mut nonce = [0u8; 24];
        rand::rng().fill_bytes(&mut nonce);
        let ct = self
            .cipher
            .encrypt(&XNonce::from(nonce), Payload { msg: plain, aad })
            .map_err(|_| MemoryError::Storage("encryption failed".into()))?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ct);
        Ok(out)
    }

    fn open_blob(&self, blob: &[u8], aad: &[u8]) -> Result<Vec<u8>, MemoryError> {
        if blob.len() < 24 {
            return Err(MemoryError::Storage("corrupt record".into()));
        }
        let (nonce, ct) = blob.split_at(24);
        let nonce: [u8; 24] = nonce
            .try_into()
            .map_err(|_| MemoryError::Storage("corrupt record".into()))?;
        self.cipher
            .decrypt(&XNonce::from(nonce), Payload { msg: ct, aad })
            .map_err(|_| MemoryError::Storage("record failed authentication".into()))
    }

    fn load(&mut self) -> Result<(), MemoryError> {
        let mut stmt = self
            .db
            .prepare("SELECT id, domain, kind, confidence, never_share, created_at, text, source FROM memories")
            .map_err(err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, f64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, Vec<u8>>(6)?,
                    r.get::<_, Vec<u8>>(7)?,
                ))
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        drop(stmt);
        for (id, domain, kind, confidence, never, created, text, source) in rows {
            let text = self.open_blob(&text, format!("mem:{id}:text").as_bytes())?;
            let source = self.open_blob(&source, format!("mem:{id}:source").as_bytes())?;
            self.store.restore(Memory {
                id: MemoryId(id as u64),
                domain: parse_domain(&domain)?,
                kind: parse_kind(&kind)?,
                text: String::from_utf8(text).map_err(err)?,
                source: String::from_utf8(source).map_err(err)?,
                confidence: confidence as f32,
                never_share_with: parse_agents(&never)?,
                created_at: created as u64,
            });
        }
        Ok(())
    }

    pub fn remember(&mut self, new: NewMemory) -> Result<MemoryId, MemoryError> {
        let id = self.store.remember(new)?;
        let m = self.store.get(Reader::User, id)?.clone();
        let text = self.seal(m.text.as_bytes(), format!("mem:{}:text", id.0).as_bytes())?;
        let source = self.seal(
            m.source.as_bytes(),
            format!("mem:{}:source", id.0).as_bytes(),
        )?;
        let res = self.db.execute(
            "INSERT INTO memories (id, domain, kind, confidence, never_share, created_at, text, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id.0 as i64,
                domain_str(m.domain),
                kind_str(m.kind),
                f64::from(m.confidence),
                agents_str(&m.never_share_with),
                m.created_at as i64,
                text,
                source
            ],
        );
        if let Err(e) = res {
            // keep memory and disk consistent
            let _ = self.store.forget(Reader::User, id);
            return Err(err(e));
        }
        Ok(id)
    }

    pub fn share(&mut self, by: Reader, id: MemoryId) -> Result<(), MemoryError> {
        self.store.share(by, id)?;
        self.db
            .execute(
                "UPDATE memories SET domain = ?1 WHERE id = ?2",
                params![domain_str(Domain::Collective), id.0 as i64],
            )
            .map_err(err)?;
        Ok(())
    }

    /// Deletes the row (with SQLite secure_delete) and every derived index entry.
    pub fn forget(&mut self, by: Reader, id: MemoryId) -> Result<(), MemoryError> {
        self.store.forget(by, id)?;
        self.db
            .execute("DELETE FROM memories WHERE id = ?1", params![id.0 as i64])
            .map_err(err)?;
        // Derived data goes with the record: its embedding can reveal content.
        self.db
            .execute("DELETE FROM kv WHERE k = ?1", params![vector_key(id)])
            .map_err(err)?;
        Ok(())
    }

    /// Stores the embedding of a record, encrypted like everything else.
    pub fn put_vector(&mut self, id: MemoryId, v: &[f32]) -> Result<(), MemoryError> {
        let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        let key = vector_key(id);
        let blob = self.seal(&bytes, format!("kv:{key}").as_bytes())?;
        self.db
            .execute(
                "INSERT INTO kv (k, v) VALUES (?1, ?2) ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                params![key, blob],
            )
            .map_err(err)?;
        Ok(())
    }

    /// The stored embedding of a record, if any.
    pub fn vector(&self, id: MemoryId) -> Result<Option<Vec<f32>>, MemoryError> {
        let key = vector_key(id);
        let blob: Option<Vec<u8>> = self
            .db
            .query_row("SELECT v FROM kv WHERE k = ?1", params![key], |r| r.get(0))
            .optional()
            .map_err(err)?;
        let Some(b) = blob else { return Ok(None) };
        let plain = self.open_blob(&b, format!("kv:{key}").as_bytes())?;
        if plain.len() % 4 != 0 {
            return Err(MemoryError::Storage("corrupt vector".into()));
        }
        Ok(Some(
            plain
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect(),
        ))
    }

    pub fn get(&self, reader: Reader, id: MemoryId) -> Result<&Memory, MemoryError> {
        self.store.get(reader, id)
    }
    pub fn search(&self, reader: Reader, query: &str) -> Vec<&Memory> {
        self.store.search(reader, query)
    }
    pub fn context_for(&self, reader: Reader) -> Vec<&str> {
        self.store.context_for(reader)
    }
    pub fn recent(&self, reader: Reader, limit: usize) -> Vec<&Memory> {
        self.store.recent(reader, limit)
    }
    pub fn len(&self) -> usize {
        self.store.len()
    }
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    /// Encrypted key/value storage for small app state (e.g. the focus list).
    pub fn put(&mut self, key: &str, value: &str) -> Result<(), MemoryError> {
        let blob = self.seal(value.as_bytes(), format!("kv:{key}").as_bytes())?;
        self.db
            .execute(
                "INSERT INTO kv (k, v) VALUES (?1, ?2) ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                params![key, blob],
            )
            .map_err(err)?;
        Ok(())
    }

    pub fn get_value(&self, key: &str) -> Result<Option<String>, MemoryError> {
        let blob: Option<Vec<u8>> = self
            .db
            .query_row("SELECT v FROM kv WHERE k = ?1", params![key], |r| r.get(0))
            .optional()
            .map_err(err)?;
        match blob {
            None => Ok(None),
            Some(b) => {
                let plain = self.open_blob(&b, format!("kv:{key}").as_bytes())?;
                Ok(Some(String::from_utf8(plain).map_err(err)?))
            }
        }
    }
}

fn vector_key(id: MemoryId) -> String {
    format!("vec:v1:{}", id.0)
}

fn domain_str(d: Domain) -> &'static str {
    match d {
        Domain::Private(AgentId::Sharee) => "private:sharee",
        Domain::Private(AgentId::Nexa) => "private:nexa",
        Domain::Collective => "collective",
    }
}
fn parse_domain(s: &str) -> Result<Domain, MemoryError> {
    Ok(match s {
        "private:sharee" => Domain::Private(AgentId::Sharee),
        "private:nexa" => Domain::Private(AgentId::Nexa),
        "collective" => Domain::Collective,
        _ => return Err(MemoryError::Storage(format!("unknown domain {s}"))),
    })
}
fn kind_str(k: Kind) -> &'static str {
    match k {
        Kind::Episodic => "episodic",
        Kind::Semantic => "semantic",
        Kind::Procedural => "procedural",
        Kind::Preference => "preference",
        Kind::Prospective => "prospective",
        Kind::Reflective => "reflective",
    }
}
fn parse_kind(s: &str) -> Result<Kind, MemoryError> {
    Ok(match s {
        "episodic" => Kind::Episodic,
        "semantic" => Kind::Semantic,
        "procedural" => Kind::Procedural,
        "preference" => Kind::Preference,
        "prospective" => Kind::Prospective,
        "reflective" => Kind::Reflective,
        _ => return Err(MemoryError::Storage(format!("unknown kind {s}"))),
    })
}
fn agents_str(set: &BTreeSet<AgentId>) -> String {
    set.iter()
        .map(|a| match a {
            AgentId::Sharee => "sharee",
            AgentId::Nexa => "nexa",
        })
        .collect::<Vec<_>>()
        .join(",")
}
fn parse_agents(s: &str) -> Result<BTreeSet<AgentId>, MemoryError> {
    s.split(',')
        .filter(|x| !x.is_empty())
        .map(|x| match x {
            "sharee" => Ok(AgentId::Sharee),
            "nexa" => Ok(AgentId::Nexa),
            _ => Err(MemoryError::Storage(format!("unknown agent {x}"))),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHAREE: Reader = Reader::Agent(AgentId::Sharee);

    fn new(domain: Domain, text: &str, never: &[AgentId]) -> NewMemory {
        NewMemory {
            domain,
            kind: Kind::Episodic,
            text: text.into(),
            source: "chat".into(),
            confidence: 0.8,
            never_share_with: never.iter().copied().collect(),
        }
    }

    fn tmp() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "nexonic-mem-{}-{}.db",
            std::process::id(),
            rand::random::<u64>()
        ));
        p
    }

    #[test]
    fn survives_reopen_with_isolation_intact() {
        let path = tmp();
        let key = generate_key();
        let id = {
            let mut m = PersistentMemory::open(&path, &key).unwrap();
            m.remember(new(Domain::Collective, "prefers dark theme", &[]))
                .unwrap();
            m.remember(new(
                Domain::Private(AgentId::Nexa),
                "salary talk",
                &[AgentId::Sharee],
            ))
            .unwrap()
        };
        let m = PersistentMemory::open(&path, &key).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m.get(SHAREE, id), Err(MemoryError::Forbidden));
        assert!(m.search(SHAREE, "salary").is_empty());
        assert_eq!(m.search(SHAREE, "dark").len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn plaintext_never_reaches_disk() {
        let path = tmp();
        let key = generate_key();
        {
            let mut m = PersistentMemory::open(&path, &key).unwrap();
            m.remember(new(Domain::Collective, "ZEBRA-SECRET-PHRASE", &[]))
                .unwrap();
            m.put("focus", "[\"ZEBRA-TASK\"]").unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        let hay = String::from_utf8_lossy(&bytes);
        assert!(!hay.contains("ZEBRA"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn wrong_key_is_refused() {
        let path = tmp();
        {
            let mut m = PersistentMemory::open(&path, &generate_key()).unwrap();
            m.remember(new(Domain::Collective, "x y", &[])).unwrap();
        }
        assert!(matches!(
            PersistentMemory::open(&path, &generate_key()),
            Err(MemoryError::Storage(_))
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn forget_removes_row() {
        let key = generate_key();
        let mut m = PersistentMemory::open_in_memory(&key).unwrap();
        let id = m
            .remember(new(Domain::Private(AgentId::Sharee), "old note", &[]))
            .unwrap();
        m.forget(Reader::User, id).unwrap();
        let n: i64 =
            m.db.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
                .unwrap();
        assert_eq!(n, 0);
        assert!(m.is_empty());
    }

    #[test]
    fn vectors_round_trip_and_go_with_the_record() {
        let key = generate_key();
        let mut m = PersistentMemory::open_in_memory(&key).unwrap();
        let id = m
            .remember(new(Domain::Private(AgentId::Nexa), "likes green tea", &[]))
            .unwrap();
        assert_eq!(m.vector(id).unwrap(), None);
        m.put_vector(id, &[0.25, -1.5, 3.0]).unwrap();
        assert_eq!(m.vector(id).unwrap(), Some(vec![0.25, -1.5, 3.0]));
        m.forget(Reader::User, id).unwrap();
        assert_eq!(m.vector(id).unwrap(), None);
        let n: i64 =
            m.db.query_row("SELECT COUNT(*) FROM kv", [], |r| r.get(0))
                .unwrap();
        assert_eq!(n, 0, "no vector left behind");
    }

    #[test]
    fn kv_round_trip_and_swap_protection() {
        let key = generate_key();
        let mut m = PersistentMemory::open_in_memory(&key).unwrap();
        m.put("a", "alpha").unwrap();
        m.put("b", "beta").unwrap();
        assert_eq!(m.get_value("a").unwrap().as_deref(), Some("alpha"));
        // moving b's ciphertext under key a must fail authentication
        m.db.execute(
            "UPDATE kv SET v = (SELECT v FROM kv WHERE k = 'b') WHERE k = 'a'",
            [],
        )
        .unwrap();
        assert!(m.get_value("a").is_err());
        assert_eq!(m.get_value("missing").unwrap(), None);
    }

    #[test]
    fn share_persists() {
        let path = tmp();
        let key = generate_key();
        let id = {
            let mut m = PersistentMemory::open(&path, &key).unwrap();
            let id = m
                .remember(new(Domain::Private(AgentId::Sharee), "rust tip", &[]))
                .unwrap();
            m.share(SHAREE, id).unwrap();
            id
        };
        let m = PersistentMemory::open(&path, &key).unwrap();
        assert!(m.get(Reader::Agent(AgentId::Nexa), id).is_ok());
        let _ = std::fs::remove_file(&path);
    }
}
