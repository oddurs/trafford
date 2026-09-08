//! Which notes are about the same thing, without asking anybody.
//!
//! 66 of this vault's 148 notes contain no link at all. Suggesting connections
//! needs a similarity signal, and the expensive answer — embeddings — means a
//! second provider (Anthropic has no embeddings endpoint), a second key, and
//! outbound traffic in a program whose peek deliberately never touches the
//! network. On 148 notes and 1.3 MB that cost should be paid on evidence.
//!
//! So this is the cheap answer, built to be measured: BM25 over the prose,
//! plus two signals a vault has that a corpus of documents does not — notes
//! sharing a tag, and notes cited together by a third.

use super::{Note, Vault};
use std::collections::HashMap;

/// Terms shorter than this carry no topic. Two characters is "of", "in", "vs".
const MIN_TERM: usize = 3;

/// A term appearing in more than this fraction of the vault says nothing about
/// which notes are alike. IDF already discounts them; this drops them outright
/// so they cannot accumulate weight in bulk.
const TOO_COMMON: f64 = 0.5;

/// …but only once enough notes have an opinion. A word in two notes out of
/// three is not evidence of commonness, it is a small vault, and applying the
/// fraction there scores every pair at zero.
const COMMON_FLOOR: usize = 8;

/// Standard BM25 knobs. `K1` is how fast repeated terms stop counting, `B` how
/// much a long note is penalised for being long.
const K1: f64 = 1.2;
const B: f64 = 0.75;

/// How much two notes sharing a tag counts, relative to prose. A vault's tags
/// are hand-applied and deliberate, so agreement between two of them is worth
/// more than agreement between two paragraphs — but not so much that tagging
/// alone decides, or every `type/reference` note would suggest every other.
const TAG_WEIGHT: f64 = 0.6;

/// How much being cited together by a third note counts. Weaker than a tag: a
/// list of links is somebody's inventory, not a claim that its entries belong
/// together.
const CO_CITATION_WEIGHT: f64 = 0.4;

/// A suggested connection.
#[derive(Debug, Clone, PartialEq)]
pub struct Similar {
    pub id: String,
    pub score: f64,
    /// Why, in the vault's own terms — the strongest few shared words or tags.
    /// A suggestion a reader cannot evaluate is one they have to take on faith.
    pub because: Vec<String>,
}

/// The vault's prose, tokenised once so a whole pass does not retokenise it per
/// pair. Built from the index, thrown away after.
pub struct Corpus {
    /// Per note: term -> count.
    counts: Vec<HashMap<String, usize>>,
    lengths: Vec<f64>,
    average_length: f64,
    /// Term -> how many notes contain it.
    document_count: HashMap<String, usize>,
    /// Tag -> how many notes wear it. A tag on most of the vault is a filing
    /// convention, not a claim that two notes belong together.
    tag_count: HashMap<String, usize>,
    ids: Vec<String>,
}

impl Corpus {
    pub fn of(vault: &Vault) -> Corpus {
        let mut counts = Vec::with_capacity(vault.notes.len());
        let mut lengths = Vec::with_capacity(vault.notes.len());
        let mut document_count: HashMap<String, usize> = HashMap::new();
        for note in &vault.notes {
            let mut here: HashMap<String, usize> = HashMap::new();
            let mut n = 0usize;
            for term in terms(note) {
                *here.entry(term).or_default() += 1;
                n += 1;
            }
            for term in here.keys() {
                *document_count.entry(term.clone()).or_default() += 1;
            }
            counts.push(here);
            lengths.push(n as f64);
        }
        let average_length = if lengths.is_empty() {
            0.0
        } else {
            lengths.iter().sum::<f64>() / lengths.len() as f64
        };
        let mut tag_count: HashMap<String, usize> = HashMap::new();
        for note in &vault.notes {
            for tag in &note.tags {
                *tag_count.entry(tag.to_lowercase()).or_default() += 1;
            }
        }
        Corpus {
            counts,
            lengths,
            average_length,
            document_count,
            tag_count,
            ids: vault.notes.iter().map(|n| n.id.clone()).collect(),
        }
    }

    /// Inverse document frequency, the part of BM25 that makes a rare word
    /// worth more than a common one.
    fn idf(&self, term: &str) -> f64 {
        let n = self.ids.len() as f64;
        let df = *self.document_count.get(term).unwrap_or(&0) as f64;
        if df == 0.0 || (df as usize >= COMMON_FLOOR && df / n > TOO_COMMON) {
            return 0.0;
        }
        ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
    }

    /// What a shared tag is worth, by the same rarity rule as a word.
    ///
    /// `topic/computability` on six notes says something. `type/reference` on
    /// ninety-two says only that this is a vault — and with a flat weight it
    /// appeared in the reason for nearly every pair the first run produced.
    fn tag_weight(&self, tag: &str) -> f64 {
        let n = self.ids.len() as f64;
        let df = *self.tag_count.get(&tag.to_lowercase()).unwrap_or(&0) as f64;
        if df == 0.0 {
            return 0.0;
        }
        TAG_WEIGHT * ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
    }

    /// Whether a tag is worth naming as a reason. The same rule a term gets:
    /// one worn by most of the vault is a filing convention, and saying
    /// "because you tagged both `#type/reference`" explains nothing.
    fn tag_is_informative(&self, tag: &str) -> bool {
        let n = self.ids.len() as f64;
        let df = *self.tag_count.get(&tag.to_lowercase()).unwrap_or(&0);
        df > 0 && !(df >= COMMON_FLOOR && df as f64 / n > TOO_COMMON)
    }

    /// BM25 of one note's terms against another, with the shared terms that
    /// contributed most.
    fn prose(&self, a: usize, b: usize) -> (f64, Vec<(String, f64)>) {
        let (query, doc) = (&self.counts[a], &self.counts[b]);
        let len = self.lengths[b];
        let mut total = 0.0;
        let mut shared: Vec<(String, f64)> = Vec::new();
        for term in query.keys() {
            let f = match doc.get(term) {
                Some(f) => *f as f64,
                None => continue,
            };
            let idf = self.idf(term);
            if idf == 0.0 {
                continue;
            }
            let denom = f + K1 * (1.0 - B + B * len / self.average_length.max(1.0));
            let score = idf * (f * (K1 + 1.0)) / denom.max(f64::EPSILON);
            total += score;
            shared.push((term.clone(), score));
        }
        shared.sort_by(|x, y| y.1.total_cmp(&x.1));
        (total, shared)
    }
}

/// Words worth comparing notes by.
///
/// Frontmatter is skipped — it is compared separately as tags, and leaving it
/// in would make every note that records a `created` date look alike. Fenced
/// code is skipped too: two notes sharing `fn`, `let` and `impl` are not about
/// the same thing.
fn terms(note: &Note) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = note.haystack.lines().peekable();
    if lines.peek().map(|l| l.trim_end()) == Some("---") {
        lines.next();
        for line in lines.by_ref() {
            if line.trim_end() == "---" {
                break;
            }
        }
    }
    let mut in_fence = false;
    for line in lines {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for word in line.split(|c: char| !c.is_alphanumeric() && c != '\'') {
            let word = word.trim_matches('\'');
            if word.chars().count() >= MIN_TERM && !word.chars().all(|c| c.is_numeric()) {
                out.push(word.to_string());
            }
        }
    }
    out
}

/// The notes most like this one, best first.
///
/// `limit` is what to return, not what to consider — everything is scored, and
/// a note is never suggested against itself.
pub fn to(vault: &Vault, corpus: &Corpus, id: &str, limit: usize) -> Vec<Similar> {
    let Some(here) = corpus.ids.iter().position(|x| x == id) else {
        return Vec::new();
    };
    let Some(note) = vault.get(id) else {
        return Vec::new();
    };
    // Everything this note points at, and everything pointing at it: already
    // connected is not a suggestion.
    let outgoing = vault.outgoing(id);
    let backlinks: Vec<&str> = vault
        .backlinks_for(id)
        .iter()
        .map(|b| b.from.as_str())
        .collect();

    let mut out: Vec<Similar> = Vec::new();
    for (other, other_id) in corpus.ids.iter().enumerate() {
        if other == here
            || outgoing.iter().any(|o| o == other_id)
            || backlinks.contains(&other_id.as_str())
        {
            continue;
        }
        let Some(other_note) = vault.get(other_id) else {
            continue;
        };
        let (prose, shared) = corpus.prose(here, other);

        let tags: Vec<&String> = note
            .tags
            .iter()
            .filter(|t| other_note.tags.iter().any(|o| o.eq_ignore_ascii_case(t)))
            .collect();
        let co_cited = co_citations(vault, id, other_id);

        let tag_score: f64 = tags.iter().map(|t| corpus.tag_weight(t)).sum();
        let score = prose + tag_score + CO_CITATION_WEIGHT * co_cited as f64;
        if score <= 0.0 {
            continue;
        }

        // Say why in the vault's own words: shared tags first, since they are
        // deliberate, then the words that carried the most weight.
        // Only tags that carried real weight are worth naming: `#type/reference`
        // on 92 of 148 notes explains nothing about why these two are alike.
        let mut ranked: Vec<(&&String, f64)> =
            tags.iter().map(|t| (t, corpus.tag_weight(t))).collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut because: Vec<String> = ranked
            .iter()
            .filter(|(t, _)| corpus.tag_is_informative(t))
            .map(|(t, _)| format!("#{t}"))
            .collect();
        because.extend(shared.iter().take(4).map(|(w, _)| w.clone()));
        because.truncate(5);

        out.push(Similar {
            id: other_id.clone(),
            score,
            because,
        });
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
    out.truncate(limit);
    out
}

/// How many notes link to both of these.
fn co_citations(vault: &Vault, a: &str, b: &str) -> usize {
    let from_a: Vec<&str> = vault
        .backlinks_for(a)
        .iter()
        .map(|x| x.from.as_str())
        .collect();
    vault
        .backlinks_for(b)
        .iter()
        .filter(|x| from_a.contains(&x.from.as_str()))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn vault_of(files: &[(&str, &str)]) -> (TempDir, Vault) {
        let dir = TempDir::with_files(files);
        let vault = Vault::open(dir.path()).unwrap();
        (dir, vault)
    }

    #[test]
    fn a_note_about_the_same_thing_scores_above_one_that_is_not() {
        let (_d, vault) = vault_of(&[
            ("a.md", "# A\nrecursion theorem kleene fixed point\n"),
            ("b.md", "# B\nkleene recursion theorem and its proof\n"),
            ("c.md", "# C\npacking list socks shoes toothbrush\n"),
        ]);
        let corpus = Corpus::of(&vault);
        let found = to(&vault, &corpus, "a.md", 5);
        assert_eq!(found[0].id, "b.md");
        assert!(
            found.iter().all(|s| s.id != "c.md") || found[0].score > found[1].score * 2.0,
            "{found:?}"
        );
    }

    /// A suggestion a reader cannot evaluate is one they have to take on faith.
    #[test]
    fn a_suggestion_says_why_in_the_vaults_own_words() {
        let (_d, vault) = vault_of(&[
            ("a.md", "# A\nchameleon husbandry humidity gradient\n"),
            ("b.md", "# B\nhumidity gradient for a chameleon enclosure\n"),
        ]);
        let corpus = Corpus::of(&vault);
        let found = to(&vault, &corpus, "a.md", 5);
        assert!(!found[0].because.is_empty());
        assert!(
            found[0]
                .because
                .iter()
                .any(|w| w == "chameleon" || w == "humidity"),
            "{:?}",
            found[0].because
        );
    }

    /// Already connected is not a suggestion, in either direction.
    #[test]
    fn a_note_already_linked_is_not_suggested() {
        let (_d, vault) = vault_of(&[
            ("a.md", "# A\nkleene recursion theorem\nsee [[b]]\n"),
            ("b.md", "# B\nkleene recursion theorem\n"),
            ("c.md", "# C\nkleene recursion theorem\nsee [[a]]\n"),
            ("d.md", "# D\nkleene recursion theorem\n"),
        ]);
        let corpus = Corpus::of(&vault);
        let found = to(&vault, &corpus, "a.md", 5);
        let ids: Vec<&str> = found.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["d.md"], "b is linked, c links back");
    }

    /// Tags are hand-applied and deliberate, so two notes wearing the same one
    /// agree about something even when their prose does not overlap.
    #[test]
    fn a_shared_tag_counts_for_something_prose_alone_would_miss() {
        let (_d, vault) = vault_of(&[
            (
                "a.md",
                "---\ntags:\n  - topic/logic\n---\n# A\nalpha beta gamma\n",
            ),
            (
                "b.md",
                "---\ntags:\n  - topic/logic\n---\n# B\ndelta epsilon zeta\n",
            ),
            ("c.md", "# C\neta theta iota\n"),
        ]);
        let corpus = Corpus::of(&vault);
        let found = to(&vault, &corpus, "a.md", 5);
        assert_eq!(found[0].id, "b.md");
        assert!(found[0].because.contains(&"#topic/logic".to_string()));
    }

    /// Frontmatter is compared as tags, not as prose. Left in, every note that
    /// records a `created` date would look like every other.
    #[test]
    fn a_shared_created_date_does_not_make_two_notes_alike() {
        let (_d, vault) = vault_of(&[
            ("a.md", "---\ncreated: 2026-03-22\n---\n# A\nalpha\n"),
            ("b.md", "---\ncreated: 2026-03-22\n---\n# B\nbeta\n"),
        ]);
        let corpus = Corpus::of(&vault);
        assert!(to(&vault, &corpus, "a.md", 5).is_empty());
    }

    /// Two notes sharing `fn`, `let` and `impl` are not about the same thing.
    #[test]
    fn code_fences_are_not_compared() {
        let (_d, vault) = vault_of(&[
            (
                "a.md",
                "# A\napples\n```rust\nfn main() { let x = 1; }\n```\n",
            ),
            (
                "b.md",
                "# B\noranges\n```rust\nfn main() { let x = 2; }\n```\n",
            ),
        ]);
        let corpus = Corpus::of(&vault);
        assert!(to(&vault, &corpus, "a.md", 5).is_empty());
    }

    #[test]
    fn a_note_is_never_suggested_against_itself() {
        let (_d, vault) = vault_of(&[("a.md", "# A\nkleene recursion theorem\n")]);
        let corpus = Corpus::of(&vault);
        assert!(to(&vault, &corpus, "a.md", 5).is_empty());
    }

    #[test]
    fn an_unknown_note_asks_for_nothing_rather_than_panicking() {
        let (_d, vault) = vault_of(&[("a.md", "# A\n")]);
        let corpus = Corpus::of(&vault);
        assert!(to(&vault, &corpus, "nope.md", 5).is_empty());
    }

    #[test]
    fn an_empty_vault_is_not_a_division_by_zero() {
        let dir = TempDir::with_files(&[]);
        let vault = Vault::open(dir.path()).unwrap();
        let corpus = Corpus::of(&vault);
        assert!(to(&vault, &corpus, "anything.md", 5).is_empty());
    }
}

/// The harness that answered 0058, kept so the question can be asked again
/// when the vault changes shape.
///
///     TRAFFORD_SPIKE_VAULT=~/vault cargo test --release orphans -- --ignored --nocapture
#[cfg(test)]
mod spike {
    #[test]
    #[ignore]
    fn how_long_the_corpus_takes() {
        let Ok(root) = std::env::var("TRAFFORD_SPIKE_VAULT") else {
            return;
        };
        let vault = super::Vault::open(std::path::Path::new(&root)).unwrap();
        let t = std::time::Instant::now();
        let corpus = super::Corpus::of(&vault);
        println!(
            "SPIKE corpus of {} notes built in {:?}",
            vault.notes.len(),
            t.elapsed()
        );
        let t = std::time::Instant::now();
        for note in vault.notes.iter().take(50) {
            let _ = super::to(&vault, &corpus, &note.id, 5);
        }
        println!("SPIKE one lookup {:?}", t.elapsed() / 50);
    }

    #[test]
    #[ignore]
    fn suggestions_for_the_orphans() {
        let Ok(root) = std::env::var("TRAFFORD_SPIKE_VAULT") else {
            println!("set TRAFFORD_SPIKE_VAULT to a real vault to run this");
            return;
        };
        let root = std::path::Path::new(&root);
        let vault = super::Vault::open(root).unwrap();
        let corpus = super::Corpus::of(&vault);
        let orphans: Vec<String> = vault
            .notes
            .iter()
            .filter(|n| n.links.is_empty())
            .map(|n| n.id.clone())
            .collect();
        println!("SPIKE {} notes with no outgoing link", orphans.len());
        let t = std::time::Instant::now();
        let mut none = 0;
        for id in orphans.iter().take(20) {
            let found = super::to(&vault, &corpus, id, 5);
            if found.is_empty() {
                none += 1;
            }
            println!("\nSPIKE {id}");
            for s in found {
                println!(
                    "SPIKE   {:6.2}  {:58}  {}",
                    s.score,
                    s.id,
                    s.because.join(", ")
                );
            }
        }
        println!("\nSPIKE {none}/20 had no candidate");
        println!("SPIKE 20 notes scored in {:?}", t.elapsed());
    }
}
