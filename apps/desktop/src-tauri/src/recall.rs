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

/// multilingual-E5 gives every pair a fairly high cosine (unrelated text
/// ~0.70–0.73, related ~0.76–0.82 in the CI test), so "related" is decided
/// relative to the rest: a memory must stand out from the typical score by
/// SEM_MARGIN and also clear SEM_FLOOR. When nothing stands out, nothing is
/// recalled by meaning (better than recalling the wrong thing).
pub const SEM_FLOOR: f32 = 0.74;
pub const SEM_MARGIN: f32 = 0.03;
/// Typical score for unrelated text, used while there are too few memories
/// to measure it.
const SEM_BASELINE_FEW: f32 = 0.72;

/// The typical (median) similarity, or SEM_BASELINE_FEW with under 5 memories.
pub fn sem_baseline(similar: &Similar) -> f32 {
    if similar.len() < 5 {
        return SEM_BASELINE_FEW;
    }
    let mut v: Vec<f32> = similar.values().copied().collect();
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

/// Points for meaning, on the same scale as keyword hits (10 per word).
fn sem_points(sim: f32, baseline: f32) -> usize {
    if sim < SEM_FLOOR || sim - baseline < SEM_MARGIN {
        return 0;
    }
    10 + ((sim - baseline) * 200.0).round() as usize
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
    let baseline = sem_baseline(similar);
    for (id, sim) in similar {
        let points = sem_points(*sim, baseline);
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

// ---------------------------------------------------------------------------
// Consolidation (M23): old conversation snippets become a few short notes.

/// A memory area is consolidated once it holds more snippets than this.
pub const CONSOLIDATE_AT: usize = 60;
/// How many of the oldest snippets are folded into notes at a time.
pub const CONSOLIDATE_BATCH: usize = 30;
const MAX_NOTES: usize = 5;
const NOTE_CHARS: usize = 220;

/// The oldest conversation snippets of the first memory area that is due,
/// never mixing areas and never touching confidential records.
pub fn consolidation_due(store: &PersistentMemory) -> Option<(Domain, Vec<(MemoryId, String)>)> {
    let all = store.recent(Reader::User, usize::MAX);
    for domain in [
        Domain::Collective,
        Domain::Private(AgentId::Sharee),
        Domain::Private(AgentId::Nexa),
    ] {
        // `recent` is newest first.
        let mut snippets: Vec<&Memory> = all
            .iter()
            .copied()
            .filter(|m| {
                m.domain == domain
                    && m.kind == Kind::Episodic
                    && m.source == "conversation"
                    && m.never_share_with.is_empty()
            })
            .collect();
        if snippets.len() > CONSOLIDATE_AT {
            snippets.reverse();
            let oldest = snippets
                .into_iter()
                .take(CONSOLIDATE_BATCH)
                .map(|m| (m.id, m.text.clone()))
                .collect();
            return Some((domain, oldest));
        }
    }
    None
}

/// The instruction for the on-device brain.
pub fn consolidation_request(lines: &[(MemoryId, String)]) -> (String, String) {
    let system = "You condense memory notes for a personal assistant. Write at most five short \
lines, each starting with \"- \". Keep only lasting facts about the user: who they are, people \
in their life, projects, plans, preferences and decisions. Use only what the notes say; never \
add, guess or embellish. Drop greetings and small talk. Write in the language of the notes."
        .to_string();
    let mut user = String::from("Notes:\n");
    for (_, t) in lines {
        user.push_str("- ");
        user.push_str(&clip(t, LINE_CHARS));
        user.push('\n');
    }
    (system, user)
}

/// The "- " lines of the brain's answer, cleaned and bounded.
pub fn parse_notes(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")))
        .map(|l| clip(l.trim(), NOTE_CHARS))
        .filter(|l| keywords(l).len() >= 2)
        .take(MAX_NOTES)
        .collect()
}

/// Stores the notes in the same area, then removes the snippets (their
/// vectors go with them). Nothing is removed unless notes were written.
pub fn apply_consolidation(
    store: &mut PersistentMemory,
    domain: Domain,
    originals: &[(MemoryId, String)],
    notes: &[String],
) -> Result<usize, String> {
    if notes.is_empty() {
        return Ok(0);
    }
    for n in notes {
        store
            .remember(NewMemory {
                domain,
                kind: Kind::Reflective,
                text: n.clone(),
                source: format!("summary of {} conversation notes", originals.len()),
                confidence: 0.7,
                never_share_with: BTreeSet::new(),
            })
            .map_err(|e| e.to_string())?;
    }
    for (id, _) in originals {
        store.forget(Reader::User, *id).map_err(|e| e.to_string())?;
    }
    Ok(originals.len())
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
        let far: Similar = [(id, 0.73)].into();
        assert!(recall(&s, AgentId::Sharee, q, &far).is_empty());
    }

    #[test]
    fn meaning_must_stand_out_from_the_rest() {
        let mut s = store();
        s.put(CONSENT_KEY, "on").unwrap();
        for t in [
            "My sister Ayesha moved to Lahore",
            "The deployment runs every Friday evening",
            "I prefer green tea in the morning",
            "Our office is on the third floor",
            "The car needs new tyres soon",
        ] {
            store_exchange(&mut s, t, &[]).unwrap();
        }
        let ids: Vec<MemoryId> = readable(&s, AgentId::Nexa, 10)
            .into_iter()
            .map(|r| r.0)
            .collect();
        // Everything scores alike (like the Roman Urdu case): nothing recalled.
        let flat: Similar = ids
            .iter()
            .zip([0.777, 0.768, 0.762, 0.765, 0.770])
            .map(|(i, v)| (*i, v))
            .collect();
        assert!(recall(&s, AgentId::Nexa, "zzz", &flat).is_empty());
        // One clearly stands out: only that one.
        let peak: Similar = ids
            .iter()
            .zip([0.72, 0.71, 0.80, 0.72, 0.70])
            .map(|(i, v)| (*i, v))
            .collect();
        let got = recall(&s, AgentId::Nexa, "zzz", &peak);
        assert_eq!(got.len(), 1);
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

    fn fill(s: &mut PersistentMemory, n: usize, reply_by: Option<AgentId>) {
        s.put(CONSENT_KEY, "on").unwrap();
        for i in 0..n {
            let replies: Vec<(AgentId, String)> = reply_by
                .map(|a| {
                    vec![(
                        a,
                        format!("Reply number {i} about the garden project plans"),
                    )]
                })
                .unwrap_or_default();
            store_exchange(s, &format!("note {i} about the garden project"), &replies).unwrap();
        }
    }

    #[test]
    fn consolidation_waits_until_an_area_is_full() {
        let mut s = store();
        fill(&mut s, CONSOLIDATE_AT, None);
        assert!(consolidation_due(&s).is_none());
        fill(&mut s, 1, None);
        let (domain, batch) = consolidation_due(&s).unwrap();
        assert_eq!(domain, Domain::Collective);
        assert_eq!(batch.len(), CONSOLIDATE_BATCH);
        assert!(
            batch[0].1.contains("note 0 "),
            "oldest first: {}",
            batch[0].1
        );
    }

    #[test]
    fn consolidation_keeps_areas_apart_and_spares_facts() {
        let mut s = store();
        store_exchange(&mut s, "Remember that my garden has tomatoes", &[]).unwrap();
        fill(&mut s, CONSOLIDATE_AT + 1, Some(AgentId::Nexa));
        // Shared area first; Nexa's private area is due too but handled separately.
        let (domain, batch) = consolidation_due(&s).unwrap();
        assert_eq!(domain, Domain::Collective);
        assert!(batch.iter().all(|(_, t)| t.starts_with("The user said")));
        let notes = vec!["The user is planning a garden project.".to_string()];
        assert_eq!(
            apply_consolidation(&mut s, domain, &batch, &notes).unwrap(),
            CONSOLIDATE_BATCH
        );
        let (domain, batch) = consolidation_due(&s).unwrap();
        assert_eq!(domain, Domain::Private(AgentId::Nexa));
        apply_consolidation(
            &mut s,
            domain,
            &batch,
            &["Nexa suggested garden plans.".into()],
        )
        .unwrap();
        assert!(consolidation_due(&s).is_none());
        // The summary of Nexa's notes stays private to Nexa.
        assert!(recall(
            &s,
            AgentId::Sharee,
            "garden plans suggested",
            &Similar::new()
        )
        .iter()
        .all(|l| !l.contains("Nexa suggested")));
        // The explicit fact is untouched.
        assert!(
            recall(&s, AgentId::Sharee, "garden tomatoes", &Similar::new())
                .contains(&"my garden has tomatoes".to_string())
        );
    }

    #[test]
    fn nothing_is_removed_without_notes() {
        let mut s = store();
        fill(&mut s, CONSOLIDATE_AT + 1, None);
        let before = s.len();
        let (domain, batch) = consolidation_due(&s).unwrap();
        assert_eq!(apply_consolidation(&mut s, domain, &batch, &[]).unwrap(), 0);
        assert_eq!(s.len(), before);
    }

    #[test]
    fn notes_are_parsed_and_bounded() {
        let text = "Here you go:\n- The user lives in Islamabad.\n- ok\n* Prefers green tea in the morning.\n- a\n- b c d\n- e f g\n- h i j\n- k l m";
        let notes = parse_notes(text);
        assert_eq!(notes[0], "The user lives in Islamabad.");
        assert_eq!(notes[1], "Prefers green tea in the morning.");
        assert!(notes.len() <= 5);
        assert!(parse_notes("no bullet lines at all").is_empty());
    }

    #[test]
    fn memory_section_only_when_there_is_something() {
        assert!(memory_section(&[]).is_none());
        let s = memory_section(&["likes tea".into()]).unwrap();
        assert!(s.contains("- likes tea"));
    }
}
