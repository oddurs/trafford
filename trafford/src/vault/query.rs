//! A small query language over the vault.
//!
//! The vault is already a typed database — 92 notes carrying `type/reference`,
//! 62 `status/active`, 1,067 open checkboxes, 405 links — and until now the
//! only way to interrogate any of it was full-text search. Obsidian's answer
//! is dataview, which this vault uses in one note out of 148: the reader is not
//! refusing to query their notes, they are refusing to write query syntax into
//! them to do it.
//!
//! So the syntax lives in the search box:
//!
//! ```text
//! type:reference status:active     property equality
//! tag:topic/computability          frontmatter or inline, one namespace
//! task:open  task:done             the checkbox state
//! orphan  broken                   what nothing links to, what links nowhere
//! links-to:"Some Note"             through the normal resolution order
//! path:01-projects                 where it lives
//! modified:>2026-08-01             mtime, not a git date
//! sort:modified  limit:20
//! ```
//!
//! Bare words fall through to the substring search that already existed, so
//! the simplest query is the one people already type.

use std::fmt;

/// What a `field:value` term asks of a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// A frontmatter key holding this value, both compared case-insensitively.
    Property { key: String, value: String },
    /// A tag, from frontmatter or written inline — one namespace, because a
    /// reader who writes `#type/reference` in prose means what the frontmatter
    /// means.
    Tag(String),
    /// Notes holding an unfinished (or finished) checkbox.
    Task(TaskState),
    /// Nothing links here.
    Orphan,
    /// Something here links nowhere.
    Broken,
    /// Links to a note, resolved the way following the link would resolve it.
    LinksTo(String),
    /// The note's path contains this, case-insensitively.
    Path(String),
    /// Modified on, before or after a date.
    Modified { when: When, date: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    On,
    Before,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Open,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Score,
    Modified,
    Title,
    Path,
}

/// A parsed query. Every filter must hold and every term must appear; there is
/// no `or`, because nobody asked for one and an unused operator is a thing to
/// get wrong.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub filters: Vec<Filter>,
    pub terms: Vec<String>,
    pub sort: Option<Sort>,
    pub limit: Option<usize>,
}

impl Query {
    /// Whether this query asks anything at all. An empty one matches nothing,
    /// rather than everything — the search box starts empty.
    pub fn is_empty(&self) -> bool {
        self.filters.is_empty() && self.terms.is_empty()
    }
}

/// Why a query could not be read.
///
/// A misspelled field is reported rather than silently returning nothing.
/// Silence is indistinguishable from a true empty result, and that is what
/// makes a query language untrustworthy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    UnknownField {
        field: String,
        known: Vec<String>,
    },
    BadValue {
        field: String,
        value: String,
        expected: &'static str,
    },
    UnclosedQuote,
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryError::UnknownField { field, known } => {
                write!(f, "no note has a property named \"{field}\"")?;
                if let Some(near) = nearest(field, known) {
                    write!(f, " — did you mean \"{near}\"?")?;
                }
                Ok(())
            }
            QueryError::BadValue {
                field,
                value,
                expected,
            } => {
                write!(f, "{field}:{value} — expected {expected}")
            }
            QueryError::UnclosedQuote => write!(f, "unclosed quote"),
        }
    }
}

/// A token, and whether it opened with a quote.
///
/// The flag matters: `"https://example.com"` is text somebody is looking for,
/// while `links-to:"Some Note"` is a field whose value happens to be quoted.
/// Without recording where the quote sat the two are indistinguishable by the
/// time they reach the parser.
struct Token {
    text: String,
    literal: bool,
}

/// Split a query into words, keeping `"quoted values"` whole.
fn tokenize(input: &str) -> Result<Vec<Token>, QueryError> {
    let mut out: Vec<Token> = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut opened_with_quote = false;
    for c in input.chars() {
        match c {
            '"' => {
                if cur.is_empty() && !quoted {
                    opened_with_quote = true;
                }
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(Token {
                        text: std::mem::take(&mut cur),
                        literal: opened_with_quote,
                    });
                }
                opened_with_quote = false;
            }
            c => cur.push(c),
        }
    }
    if quoted {
        return Err(QueryError::UnclosedQuote);
    }
    if !cur.is_empty() {
        out.push(Token {
            text: cur,
            literal: opened_with_quote,
        });
    }
    Ok(out)
}

/// The field names that are not frontmatter keys.
const RESERVED: &[&str] = &[
    "tag", "tags", "task", "links-to", "linksto", "path", "modified", "sort", "limit",
];

/// What names this vault answers to: every frontmatter key it uses, and every
/// namespace its tags are grouped under.
///
/// Both are needed to tell a typo apart from a key this particular vault
/// happens not to have — and the second matters more than it looks. The vault
/// this was built for keeps its whole schema in namespaced tags:
/// `type/*` on 109 notes, `status/*` on 70, `priority/*` on 29, `topic/*` on
/// 12, with only `created`, `tags` and `source` as real keys. A query language
/// that read `type:reference` as a missing property would be wrong about the
/// vault it is standing in.
#[derive(Debug, Clone, Default)]
pub struct Vocabulary {
    pub properties: Vec<String>,
    pub namespaces: Vec<String>,
}

impl Vocabulary {
    fn knows_property(&self, key: &str) -> bool {
        self.properties.iter().any(|k| k.eq_ignore_ascii_case(key))
    }

    fn knows(&self, key: &str) -> bool {
        self.knows_property(key) || self.knows_namespace(key)
    }

    fn knows_namespace(&self, key: &str) -> bool {
        self.namespaces.iter().any(|k| k.eq_ignore_ascii_case(key))
    }

    fn all(&self) -> Vec<String> {
        let mut v = self.properties.clone();
        v.extend(self.namespaces.iter().cloned());
        v.sort();
        v.dedup();
        v
    }
}

/// Parse a query against what the vault knows itself to contain.
pub fn parse(input: &str, vocab: &Vocabulary) -> Result<Query, QueryError> {
    let mut q = Query::default();
    for token in tokenize(input)? {
        // A quoted token is text, whatever punctuation is inside it.
        if token.literal {
            q.terms.push(token.text.to_lowercase());
            continue;
        }
        // A bare word, or a `word:` with nothing after it, is text to look for.
        let Some((field, value)) = token.text.split_once(':') else {
            q.terms.push(token.text.to_lowercase());
            continue;
        };
        if value.is_empty() {
            q.terms.push(token.text.to_lowercase());
            continue;
        }
        let lower = field.to_ascii_lowercase();
        // A colon is punctuation far more often than it is syntax. This vault
        // holds 2,744 colon-bearing tokens in prose — every URL, every `12:30`,
        // every `TODO:` — and reading them as fields made all of them
        // unsearchable, mid-keystroke, the moment the colon landed.
        //
        // So only a name the vault actually answers to is a field. A name that
        // is *nearly* one is still reported, which is what keeps
        // `stauts:active` from silently becoming a text search for nothing.
        if !RESERVED.contains(&lower.as_str()) && !vocab.knows(&lower) {
            if let Some(_near) = nearest(&lower, &vocab.all()) {
                return Err(QueryError::UnknownField {
                    field: field.to_string(),
                    known: vocab.all(),
                });
            }
            q.terms.push(token.text.to_lowercase());
            continue;
        }
        match lower.as_str() {
            "tag" | "tags" => q
                .filters
                .push(Filter::Tag(value.trim_start_matches('#').to_string())),
            "task" => match value.to_ascii_lowercase().as_str() {
                "open" | "todo" | "unfinished" => q.filters.push(Filter::Task(TaskState::Open)),
                "done" | "complete" | "finished" => q.filters.push(Filter::Task(TaskState::Done)),
                _ => {
                    return Err(QueryError::BadValue {
                        field: lower,
                        value: value.to_string(),
                        expected: "open or done",
                    })
                }
            },
            "links-to" | "linksto" => q.filters.push(Filter::LinksTo(value.to_string())),
            "path" => q.filters.push(Filter::Path(value.to_lowercase())),
            "modified" => {
                // A bare date means that day. Reading it as "after" answers a
                // question nobody asked and excludes the one day they named,
                // which returns plausible results rather than an error.
                let (when, date) = match value.strip_prefix('>') {
                    Some(d) => (When::After, d),
                    None => match value.strip_prefix('<') {
                        Some(d) => (When::Before, d),
                        None => (When::On, value),
                    },
                };
                if !looks_like_a_date(date) {
                    return Err(QueryError::BadValue {
                        field: lower,
                        value: value.to_string(),
                        expected: "a date like >2026-08-01",
                    });
                }
                q.filters.push(Filter::Modified {
                    when,
                    date: date.to_string(),
                });
            }
            "sort" => {
                q.sort = Some(match value.to_ascii_lowercase().as_str() {
                    "modified" | "recent" => Sort::Modified,
                    "title" | "name" => Sort::Title,
                    "path" => Sort::Path,
                    "score" | "relevance" => Sort::Score,
                    _ => {
                        return Err(QueryError::BadValue {
                            field: lower,
                            value: value.to_string(),
                            expected: "modified, title, path or score",
                        })
                    }
                })
            }
            "limit" => {
                let n: usize = value.parse().map_err(|_| QueryError::BadValue {
                    field: lower.clone(),
                    value: value.to_string(),
                    expected: "a number",
                })?;
                // `limit:0` returns nothing, which is indistinguishable from a
                // query that matched nothing.
                if n == 0 {
                    return Err(QueryError::BadValue {
                        field: lower,
                        value: value.to_string(),
                        expected: "a number above zero",
                    });
                }
                q.limit = Some(n)
            }
            // A real frontmatter key.
            _ if vocab.knows_property(&lower) => q.filters.push(Filter::Property {
                key: lower,
                value: value.to_string(),
            }),
            // A tag namespace: `type:reference` is how this vault spells
            // `tag:type/reference`, and it is the spelling a reader reaches for
            // first.
            _ if vocab.knows_namespace(&lower) => {
                q.filters.push(Filter::Tag(format!("{lower}/{value}")))
            }
            // Neither. Saying so is the whole point — an empty result would
            // look like an answer.
            _ => {
                return Err(QueryError::UnknownField {
                    field: field.to_string(),
                    known: vocab.all(),
                })
            }
        }
    }
    // `orphan` and `broken` are bare words rather than fields, so they arrive
    // as terms and are promoted here. A note actually containing the word
    // "orphan" is then unfindable by typing it, which is why they are the only
    // two and both are vocabulary a vault is unlikely to use as prose.
    q.terms.retain(|t| match t.as_str() {
        "orphan" | "orphans" => {
            q.filters.push(Filter::Orphan);
            false
        }
        "broken" => {
            q.filters.push(Filter::Broken);
            false
        }
        _ => true,
    });
    Ok(q)
}

/// `YYYY-MM-DD`, which is the only form worth accepting: it sorts as a string,
/// it is what the vault's own frontmatter writes, and it is unambiguous
/// between readers who write months first and readers who do not.
fn looks_like_a_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// The closest known field by edit distance, when it is close enough to be
/// worth suggesting. A suggestion that is nowhere near is worse than none.
fn nearest<'a>(field: &str, known: &'a [String]) -> Option<&'a str> {
    let field = field.to_ascii_lowercase();
    known
        .iter()
        .map(|k| (distance(&field, &k.to_ascii_lowercase()), k))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k.as_str())
}

/// Levenshtein distance, two rows at a time.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.is_empty() {
        return b.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Whether a line is a checkbox, and whether it is ticked.
///
/// Indented ones count: the vault's checklists nest, and a sub-task is a task.
pub fn checkbox(line: &str) -> Option<bool> {
    let t = line.trim_start();
    let rest = t.strip_prefix("- ").or_else(|| t.strip_prefix("* "))?;
    let inner = rest.strip_prefix('[')?;
    let mark = inner.chars().next()?;
    if inner.chars().nth(1) != Some(']') {
        return None;
    }
    match mark {
        ' ' => Some(false),
        'x' | 'X' => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> Vocabulary {
        Vocabulary {
            properties: vec!["created".into(), "tags".into(), "status".into()],
            namespaces: vec!["type".into(), "topic".into()],
        }
    }

    #[test]
    fn bare_words_are_text_to_look_for() {
        let q = parse("kleene recursion", &known()).unwrap();
        assert_eq!(q.terms, vec!["kleene", "recursion"]);
        assert!(q.filters.is_empty());
    }

    #[test]
    fn a_property_becomes_a_filter() {
        let q = parse("status:active", &known()).unwrap();
        assert_eq!(
            q.filters,
            vec![Filter::Property {
                key: "status".into(),
                value: "active".into()
            }]
        );
    }

    /// The vault this was built for keeps its schema in namespaced tags, so
    /// `type:reference` has to mean `tag:type/reference`. Reading it as a
    /// missing property would be wrong about the vault it is standing in.
    #[test]
    fn a_tag_namespace_reads_like_a_property() {
        let q = parse("type:reference", &known()).unwrap();
        assert_eq!(q.filters, vec![Filter::Tag("type/reference".into())]);
    }

    /// A real key wins over a namespace of the same name: the key is what an
    /// author wrote deliberately, the namespace is a convention.
    #[test]
    fn a_real_key_beats_a_namespace_that_shares_its_name() {
        let vocab = Vocabulary {
            properties: vec!["status".into()],
            namespaces: vec!["status".into()],
        };
        let q = parse("status:active", &vocab).unwrap();
        assert!(matches!(q.filters[0], Filter::Property { .. }));
    }

    #[test]
    fn a_misspelled_field_is_reported_rather_than_returning_nothing() {
        // An empty result is indistinguishable from a true empty result, which
        // is the failure that makes a query language untrustworthy.
        let err = parse("stauts:active", &known()).unwrap_err();
        assert!(matches!(err, QueryError::UnknownField { .. }));
        assert!(err.to_string().contains("did you mean \"status\""), "{err}");
    }

    /// The cost of letting colons through, stated plainly: a field name that
    /// resembles nothing the vault knows becomes a text search rather than an
    /// error.
    ///
    /// This is the deliberate half of the trade. `assignee:me` looks like a
    /// mistake to a person and like prose to the parser, and there is no way
    /// to tell it from `TODO:fix` without knowing what the reader meant. The
    /// alternative — erroring on every unknown `word:word` — made 2,744 real
    /// tokens in this vault unsearchable, which is the worse failure by three
    /// orders of magnitude.
    #[test]
    fn a_field_resembling_nothing_becomes_a_text_search() {
        let q = parse("assignee:me", &known()).unwrap();
        assert_eq!(q.terms, vec!["assignee:me"]);
        assert!(q.filters.is_empty());
    }

    #[test]
    fn quoted_values_survive_their_spaces() {
        let q = parse("links-to:\"Some Note\"", &known()).unwrap();
        assert_eq!(q.filters, vec![Filter::LinksTo("Some Note".into())]);
    }

    #[test]
    fn an_unclosed_quote_is_an_error_not_a_guess() {
        assert_eq!(
            parse("links-to:\"oops", &known()),
            Err(QueryError::UnclosedQuote)
        );
    }

    #[test]
    fn task_state_parses_both_ways_and_rejects_the_rest() {
        assert_eq!(
            parse("task:open", &known()).unwrap().filters,
            vec![Filter::Task(TaskState::Open)]
        );
        assert_eq!(
            parse("task:done", &known()).unwrap().filters,
            vec![Filter::Task(TaskState::Done)]
        );
        assert!(parse("task:maybe", &known()).is_err());
    }

    #[test]
    fn orphan_and_broken_are_bare_words() {
        let q = parse("orphan", &known()).unwrap();
        assert_eq!(q.filters, vec![Filter::Orphan]);
        assert!(q.terms.is_empty());
        let q = parse("broken", &known()).unwrap();
        assert_eq!(q.filters, vec![Filter::Broken]);
    }

    #[test]
    fn modified_takes_a_direction_and_a_real_date() {
        let q = parse("modified:>2026-08-01", &known()).unwrap();
        assert_eq!(
            q.filters,
            vec![Filter::Modified {
                when: When::After,
                date: "2026-08-01".into()
            }]
        );
        assert!(parse("modified:yesterday", &known()).is_err());
        assert!(parse("modified:>08-01", &known()).is_err());
    }

    /// A bare date means that day. Reading it as "after" answers a question
    /// nobody asked, and excludes the single day they named.
    #[test]
    fn a_bare_date_means_that_day() {
        let q = parse("modified:2026-08-01", &known()).unwrap();
        assert_eq!(
            q.filters,
            vec![Filter::Modified {
                when: When::On,
                date: "2026-08-01".into()
            }]
        );
    }

    /// A colon is punctuation far more often than it is syntax. The vault this
    /// was built for holds 2,744 colon-bearing tokens in prose, and reading
    /// them all as fields made every URL unsearchable — mid-keystroke, the
    /// moment the colon landed.
    #[test]
    fn text_containing_a_colon_is_still_text() {
        for input in ["https://example.com", "TODO:fix", "12:30", "note:3"] {
            let q = parse(input, &known())
                .unwrap_or_else(|e| panic!("{input:?} should search, not fail: {e}"));
            assert_eq!(q.terms, vec![input.to_lowercase()], "{input:?}");
            assert!(q.filters.is_empty(), "{input:?}");
        }
    }

    /// But a near-miss is still a mistake worth naming, or the fix above would
    /// turn every typo into a silent search for nothing.
    #[test]
    fn a_near_miss_is_still_reported_after_the_colon_fix() {
        assert!(parse("stauts:active", &known()).is_err());
        assert!(parse("tagz:x", &known()).is_err());
    }

    /// A quoted token is text whatever punctuation is inside it, and the quote
    /// has to be remembered for that to be knowable.
    #[test]
    fn a_quoted_token_is_text_even_when_it_looks_like_a_field() {
        let q = parse("\"status:active\"", &known()).unwrap();
        assert_eq!(q.terms, vec!["status:active"]);
        assert!(q.filters.is_empty());
        // The value-side quote still parses as a field.
        let q = parse("links-to:\"Some Note\"", &known()).unwrap();
        assert_eq!(q.filters, vec![Filter::LinksTo("Some Note".into())]);
    }

    /// `limit:0` returns nothing, which is indistinguishable from a query that
    /// matched nothing.
    #[test]
    fn a_limit_of_zero_is_a_mistake_not_an_answer() {
        assert!(parse("limit:0", &known()).is_err());
        assert!(parse("limit:1", &known()).is_ok());
    }

    #[test]
    fn sort_and_limit_are_not_filters() {
        let q = parse("type:reference sort:modified limit:5", &known()).unwrap();
        assert_eq!(q.sort, Some(Sort::Modified));
        assert_eq!(q.limit, Some(5));
        assert_eq!(q.filters.len(), 1);
        assert!(parse("limit:lots", &known()).is_err());
    }

    #[test]
    fn a_word_with_a_trailing_colon_is_text_not_a_broken_field() {
        // "TODO:" appears in prose, and refusing to search for it would be a
        // worse answer than searching for it.
        let q = parse("todo:", &known()).unwrap();
        assert_eq!(q.terms, vec!["todo:"]);
        assert!(q.filters.is_empty());
    }

    #[test]
    fn an_empty_query_asks_nothing() {
        let q = parse("   ", &known()).unwrap();
        assert!(q.is_empty());
    }

    #[test]
    fn parsing_never_panics_on_punctuation() {
        for input in [
            "::", ":", "\"", "a:\"b", "-", "sort:", "limit:-1", "[]", "a::b",
        ] {
            let _ = parse(input, &known());
        }
    }

    #[test]
    fn checkboxes_are_recognised_including_nested_ones() {
        assert_eq!(checkbox("- [ ] a task"), Some(false));
        assert_eq!(checkbox("    - [x] done"), Some(true));
        assert_eq!(checkbox("* [X] star form"), Some(true));
        assert_eq!(checkbox("- [-] not a checkbox"), None);
        assert_eq!(checkbox("- plain item"), None);
        assert_eq!(checkbox("text [ ] mid-line"), None);
    }
}
