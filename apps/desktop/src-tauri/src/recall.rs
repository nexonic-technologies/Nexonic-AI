//! Conversation memory for Sharee and Nexa, on top of the encrypted store.
//!
//! * Nothing from a conversation is kept unless the user turns on
//!   "Remember our conversations" (off by default).
//! * "Remember that …" is an explicit request, so it is kept even when that
//!   switch is off. Addressed to one agent ("Sharee, remember that …") it goes
//!   into that agent's private memory, otherwise into shared memory.
//! * What the user said goes into shared memory; each agent's own reply goes
//!   into its private memory, so neither agent reads the other's notes.
//! * Before replying, an agent recalls only records it may read (the store
//!   enforces that), ranked by overlap with the user's message.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use nexonic_memory_engine::persist::PersistentMemory;
use nexonic_memory_engine::{AgentId, Domain, Kind, Memory, MemoryId, NewMemory, Reader};

/// Cosine similarity (query ↔ memory) from the on-device embedding model.
/// Empty when the model is not available; recall then uses words only.
pub type Similar = HashMap<MemoryId, f32>;

/// Below this cosine a memory is not considered related (multilingual-E5
/// scores unrelated text around 0.75–0.80).
pub const SEM_MIN: f32 = 0.82;

/// Points for meaning, on the same scale as keyword hits (10 per word).
fn sem_points(sim: f32) -> usize {
    if sim < SEM_MIN {
        return 0;
    }
    10 + ((sim - SEM_MIN) * 400.0).round() as usize
}

/// kv key for the consent switch.
pub const CONSENT_KEY: &str = "memory.conversations";

/// How much recalled text an agent gets at most.
const RECALL_CHARS: usize = 1_200;
const RECALL_ITEMS: usize = 6;
/// Longest single remembered line.
const LINE_CHARS: usize = 320;

/// Common words that say nothing about what a message is about.
const STOP: &[&str] = &[
    "the", "and", "for", "you", "your", "are", "was", "were", "with", "that", "this", "what",
    "how", "can", "could", "would", "should", "about", "have", "has", "had", "not", "but", "all",
    "any", "our", "out", "who", "why", "when", "where", "which", "will", "just", "from", "they",
    "them", "then", "than", "there", "here", "into", "also", "some", "tell", "please", "me", "my",
    "is", "it", "to", "of", "in", "on", "a", "an", "do", "does", "did", "be", "am", "i", "we",
    "hi", "hello", "hey", "ok", "okay", "yes", "no", "sharee", "nexa", "both", "said", "replied",
    "kya", "hai", "ho", "ka", "ki", "ke", "ko", "se", "mein", "main", "aur", "bhi", "tum", "aap",
];

pub fn agent_id(name: &str) -> Option<AgentId> {
    match name {
        "Sharee" => Some(AgentId::Sharee),
        "Nexa" => Some(AgentId::Nexa),
        _ => None,
    }
}

fn agent_name(a: AgentId) -> &'static str {
    match a {
        AgentId::Sharee => "Sharee",
        AgentId::Nexa => "Nexa",
    }
}

fn keywords(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() > 2 && !STOP.contains(&w.as_str()))
        .collect()
}

/// Shortens on a character boundary, adding an ellipsis.
pub fn clip(text: &str, max: usize) -> String {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= max {
        return t;
    }
    let mut out: String = t.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// "remember that X", "remember: X", "please remember X", "yaad rakhna X".
/// Returns the fact and, when the message is addressed to one agent, that agent.
pub fn explicit_fact(message: &str) -> Option<(String, Option<AgentId>)> {
    let lower = message.to_lowercase();
    let triggers = [
        "remember that ",
        "remember: ",
        "please remember ",
        "remember ",
        "yaad rakhna ",
        "yaad rakho ",
        "yaad rakh ",
    ];
    let (pos, len) = triggers
        .iter()
        .filter_map(|t| lower.find(t).map(|p| (p, t.len())))
        .min_by_key(|(p, _)| *p)?;
    // Only at the start of the message or right after addressing an agent,
    // so "do you remember …" is a question, not an instruction.
    let before = lower[..pos].trim().trim_end_matches([',', ':', '!']).trim();
    let to = match before {
        "" | "please" => None,
        "sharee" | "hey sharee" | "sharee please" => Some(AgentId::Sharee),
        "nexa" | "hey nexa" | "nexa please" => Some(AgentId::Nexa),
        _ => return None,
    };
    let fact = message[pos + len..]
        .trim()
        .trim_end_matches(['.', '!'])
        .trim();
    if fact.chars().count() < 3 {
        return None;
    }
    Some((clip(fact, LINE_CHARS), to))
}

fn score(m: &Memory, query: &HashSet<String>) -> usize {
    let words = keywords(&m.text);
    let hits = query.intersection(&words).count();
    if hits == 0 {
        return 0;
    }
    // Facts the user asked to keep outrank conversation snippets.
    let weight = match m.kind {
        Kind::Semantic | Kind::Preference => 3,
        _ => 1,
    };
    hits * 10 * weight
}

/// Memories `agent` may read that relate to `message`, best first: words in
/// common plus, when the embedding model is available, closeness in meaning.
pub fn recall(
    store: &PersistentMemory,
    agent: AgentId,
    message: &str,
    similar: &Similar,
) -> Vec<String> {
    let query = keywords(message);
    let reader = Reader::Agent(agent);
    // id → (score, created_at, text)
    let mut found: BTreeMap<MemoryId, (usize, u64, String)> = BTreeMap::new();
    if !query.is_empty() {
        for m in store.search(reader, message) {
            let s = score(m, &query);
            if s > 0 {
                found.insert(m.id, (s, m.created_at, m.text.clone()));
            }
        }
    }
    for (id, sim) in similar {
        let points = sem_points(*sim);
        if points == 0 {
            continue;
        }
        // Re-checked here: only records this agent may read.
        if let Ok(m) = store.get(reader, *id) {
            found.entry(*id).and_modify(|e| e.0 += points).or_insert((
                points,
                m.created_at,
                m.text.clone(),
            ));
        }
    }
    // Facts the user asked an agent to keep are always in view (they are few
    // and short); conversation snippets only when they relate.
    for m in store.recent(reader, 200) {
        if matches!(m.kind, Kind::Semantic | Kind::Preference) && m.source == "user asked" {
            found
                .entry(m.id)
                .or_insert((1, m.created_at, m.text.clone()));
        }
    }
    let mut scored: Vec<(usize, u64, String)> = found.into_values().collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let mut out = Vec::new();
    let mut used = 0;
    for (_, _, text) in scored.into_iter().take(RECALL_ITEMS * 2) {
        let line = clip(&text, LINE_CHARS);
        if used + line.len() > RECALL_CHARS || out.len() == RECALL_ITEMS {
            break;
        }
        used += line.len();
        out.push(line);
    }
    out
}

/// Records `agent` may read, for embedding: (id, text).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn readable(store: &PersistentMemory, agent: AgentId, limit: usize) -> Vec<(MemoryId, String)> {
    store
        .recent(Reader::Agent(agent), limit)
        .into_iter()
        .map(|m| (m.id, m.text.clone()))
        .collect()
}

/// The system-prompt section listing what the agent remembers.
pub fn memory_section(lines: &[String]) -> Option<String> {
    if lines.is_empty() {
        return None;
    }
    let mut s = String::from(
        "\n\nThings you remember from earlier conversations with the user (stored on their Mac). \
Use them only when they help with the current message, never invent anything beyond them, and \
don't list them back unless asked:",
    );
    for l in lines {
        s.push_str("\n- ");
        s.push_str(l);
    }
    Some(s)
}

pub fn consent(store: &PersistentMemory) -> bool {
    matches!(store.get_value(CONSENT_KEY), Ok(Some(v)) if v == "on")
}

/// Stores one exchange. Returns the number of records written.
pub fn store_exchange(
    store: &mut PersistentMemory,
    user_text: &str,
    replies: &[(AgentId, String)],
) -> Result<usize, String> {
    let mut written = 0;
    let explicit = explicit_fact(user_text);
    if let Some((fact, to)) = &explicit {
        store
            .remember(NewMemory {
                domain: to.map_or(Domain::Collective, Domain::Private),
                kind: Kind::Semantic,
                text: fact.clone(),
                source: "user asked".into(),
                confidence: 1.0,
                never_share_with: BTreeSet::new(),
            })
            .map_err(|e| e.to_string())?;
        written += 1;
    }
    if !consent(store) {
        return Ok(written);
    }
    // The explicit fact already holds what matters in that message.
    if explicit.is_none() && keywords(user_text).len() >= 2 {
        store
            .remember(NewMemory {
                domain: Domain::Collective,
                kind: Kind::Episodic,
                text: format!("The user said: {}", clip(user_text, LINE_CHARS)),
                source: "conversation".into(),
                confidence: 0.9,
                never_share_with: BTreeSet::new(),
            })
            .map_err(|e| e.to_string())?;
        written += 1;
    }
    for (agent, reply) in replies {
        if keywords(reply).len() < 3 {
            continue;
        }
        store
            .remember(NewMemory {
                domain: Domain::Private(*agent),
                kind: Kind::Episodic,
                text: format!(
                    "I ({}) replied to \"{}\": {}",
                    agent_name(*agent),
                    clip(user_text, 80),
                    clip(reply, 220)
                ),
                source: "conversation".into(),
                confidence: 0.8,
                never_share_with: BTreeSet::new(),
            })
            .map_err(|e| e.to_string())?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexonic_memory_engine::persist::generate_key;

    fn store() -> PersistentMemory {
        PersistentMemory::open_in_memory(&generate_key()).unwrap()
    }

    #[test]
    fn explicit_facts_are_recognised() {
        assert_eq!(
            explicit_fact("Remember that my sister is called Ayesha."),
            Some(("my sister is called Ayesha".into(), None))
        );
        assert_eq!(
            explicit_fact("Sharee, remember: I deploy on Fridays"),
            Some(("I deploy on Fridays".into(), Some(AgentId::Sharee)))
        );
        assert_eq!(
            explicit_fact("nexa yaad rakhna meri meeting 5 baje hai"),
            Some(("meri meeting 5 baje hai".into(), Some(AgentId::Nexa)))
        );
        assert_eq!(explicit_fact("Do you remember what I said?"), None);
        assert_eq!(explicit_fact("remember"), None);
    }

    #[test]
    fn nothing_from_conversation_is_kept_without_consent() {
        let mut s = store();
        let n = store_exchange(
            &mut s,
            "I am building a WordPress plugin for news",
            &[(
                AgentId::Sharee,
                "Great, start with the custom post type and a REST route.".into(),
            )],
        )
        .unwrap();
        assert_eq!(n, 0);
        assert!(s.is_empty());
    }

    #[test]
    fn explicit_facts_are_kept_even_without_consent() {
        let mut s = store();
        let n = store_exchange(&mut s, "Remember that my favourite colour is teal", &[]).unwrap();
        assert_eq!(n, 1);
        let got = recall(
            &s,
            AgentId::Nexa,
            "what colour should the logo be?",
            &Similar::new(),
        );
        assert_eq!(got, vec!["my favourite colour is teal".to_string()]);
    }

    #[test]
    fn private_replies_stay_private() {
        let mut s = store();
        s.put(CONSENT_KEY, "on").unwrap();
        store_exchange(
            &mut s,
            "Plan the launch of the Nexonic website",
            &[
                (
                    AgentId::Sharee,
                    "Launch checklist: domain, hosting, analytics and a press page.".into(),
                ),
                (
                    AgentId::Nexa,
                    "Let's add a teaser video and a countdown banner for the launch.".into(),
                ),
            ],
        )
        .unwrap();
        let sharee = recall(
            &s,
            AgentId::Sharee,
            "what about the launch?",
            &Similar::new(),
        )
        .join("\n");
        let nexa = recall(&s, AgentId::Nexa, "what about the launch?", &Similar::new()).join("\n");
        assert!(sharee.contains("The user said: Plan the launch"));
        assert!(sharee.contains("checklist"));
        assert!(
            !sharee.contains("teaser"),
            "Sharee must not read Nexa's private notes"
        );
        assert!(nexa.contains("teaser"));
        assert!(
            !nexa.contains("checklist"),
            "Nexa must not read Sharee's private notes"
        );
    }

    #[test]
    fn a_fact_told_to_one_agent_is_private_to_it() {
        let mut s = store();
        store_exchange(
            &mut s,
            "Sharee, remember that the server password rotates monthly",
            &[],
        )
        .unwrap();
        assert!(!recall(&s, AgentId::Sharee, "server password", &Similar::new()).is_empty());
        assert!(recall(&s, AgentId::Nexa, "server password", &Similar::new()).is_empty());
    }

    #[test]
    fn unrelated_snippets_are_not_recalled() {
        let mut s = store();
        s.put(CONSENT_KEY, "on").unwrap();
        store_exchange(&mut s, "Help me choose a laptop for video editing", &[]).unwrap();
        assert!(recall(
            &s,
            AgentId::Sharee,
            "write a poem about rain",
            &Similar::new()
        )
        .is_empty());
    }

    #[test]
    fn recall_is_bounded() {
        let mut s = store();
        s.put(CONSENT_KEY, "on").unwrap();
        for i in 0..40 {
            store_exchange(
                &mut s,
                &format!("project alpha milestone {i} needs testing and review"),
                &[],
            )
            .unwrap();
        }
        let got = recall(&s, AgentId::Nexa, "project alpha testing", &Similar::new());
        assert!(got.len() <= RECALL_ITEMS);
        assert!(got.iter().map(String::len).sum::<usize>() <= RECALL_CHARS);
    }

    #[test]
    fn meaning_finds_what_words_miss() {
        let mut s = store();
        s.put(CONSENT_KEY, "on").unwrap();
        store_exchange(&mut s, "My sister Ayesha moved to Lahore last month", &[]).unwrap();
        let id = readable(&s, AgentId::Sharee, 10)[0].0;
        let q = "what is my sibling's name?";
        assert!(recall(&s, AgentId::Sharee, q, &Similar::new()).is_empty());
        let close: Similar = [(id, 0.88)].into();
        assert_eq!(recall(&s, AgentId::Sharee, q, &close).len(), 1);
        let far: Similar = [(id, 0.78)].into();
        assert!(recall(&s, AgentId::Sharee, q, &far).is_empty());
    }

    #[test]
    fn meaning_never_crosses_privacy() {
        let mut s = store();
        s.put(CONSENT_KEY, "on").unwrap();
        store_exchange(
            &mut s,
            "hello there friend",
            &[(
                AgentId::Nexa,
                "Nexa's private thought about the surprise party plans".into(),
            )],
        )
        .unwrap();
        let nexa_ids: Vec<MemoryId> = readable(&s, AgentId::Nexa, 10)
            .into_iter()
            .map(|r| r.0)
            .collect();
        // Even a perfect similarity for Nexa's private record must not reach Sharee.
        let leak: Similar = nexa_ids.iter().map(|id| (*id, 0.99)).collect();
        let got = recall(&s, AgentId::Sharee, "party", &leak).join("\n");
        assert!(!got.contains("surprise party"));
    }

    #[test]
    fn memory_section_only_when_there_is_something() {
        assert!(memory_section(&[]).is_none());
        let s = memory_section(&["likes tea".into()]).unwrap();
        assert!(s.contains("- likes tea"));
    }
}
