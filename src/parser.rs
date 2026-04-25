pub struct Command {
    pub verb: String,
    pub noun: Option<String>,
}

/// Find the best match for `query` in `candidates`.
///
/// Resolution order:
///   1. Exact match (after normalising underscores → spaces)
///   2. Candidate name contains the whole query as a substring
///      ("journal" → "leather_journal")
///   3. Any query word is a prefix of a candidate word, min length 3
///      ("photo" → "old_photograph")
///   4. Levenshtein edit-distance on individual words (typo tolerance)
///      ("journl" → "leather_journal", "rign" → "signet_ring")
///
/// Returns `None` if no confident match is found.
pub fn fuzzy_match<'a>(query: &str, candidates: &'a [String]) -> Option<&'a String> {
    if candidates.is_empty() || query.is_empty() {
        return None;
    }

    let q = query.replace('_', " ").to_lowercase();

    // 1. Exact
    for c in candidates {
        if c.replace('_', " ").to_lowercase() == q {
            return Some(c);
        }
    }

    // 2. Candidate contains query as substring
    for c in candidates {
        if c.replace('_', " ").to_lowercase().contains(&q) {
            return Some(c);
        }
    }

    // 3. Any query word is a prefix of any candidate word (min 3 chars)
    let q_words: Vec<&str> = q.split_whitespace().collect();
    for c in candidates {
        let cn = c.replace('_', " ").to_lowercase();
        let c_words: Vec<&str> = cn.split_whitespace().collect();
        if q_words.iter().any(|qw| {
            qw.len() >= 3 && c_words.iter().any(|cw| cw.starts_with(*qw))
        }) {
            return Some(c);
        }
    }

    // 4. Edit-distance match — only for query words of length >= 3
    let meaningful: Vec<&str> = q_words.iter()
        .copied()
        .filter(|w| w.len() >= 3)
        .collect();

    if meaningful.is_empty() {
        return None;
    }

    let threshold = |len: usize| -> usize {
        match len {
            0..=4 => 1,
            5..=7 => 2,
            _     => 3,
        }
    };

    let mut best: Option<&String> = None;
    let mut best_dist = usize::MAX;

    for c in candidates {
        let cn = c.replace('_', " ").to_lowercase();
        let c_words: Vec<&str> = cn.split_whitespace().collect();
        for qw in &meaningful {
            for cw in &c_words {
                let d = edit_distance(qw, cw);
                if d < best_dist {
                    best_dist = d;
                    best = Some(c);
                }
            }
        }
    }

    if let Some(m) = best {
        let min_len = meaningful.iter().map(|w| w.len()).min().unwrap_or(0);
        if best_dist <= threshold(min_len) {
            return Some(m);
        }
    }

    None
}

/// Levenshtein edit distance between two strings.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    if m == 0 { return n; }
    if n == 0 { return m; }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            curr[j] = if a[i - 1] == b[j - 1] {
                prev[j - 1]
            } else {
                1 + prev[j].min(curr[j - 1]).min(prev[j - 1])
            };
        }
        prev.clone_from(&curr);
    }

    prev[n]
}

/// Parse `ask [npc] about <topic>` and `tell [npc] about <topic>`.
///
/// Returns `(npc_name, topic)` where `npc_name` is `None` if the player
/// didn't name anyone (use whoever is in the current room).
/// Returns `None` if "about" is absent or the topic is empty.
pub fn parse_ask(input: &str) -> Option<(Option<String>, String)> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.is_empty() { return None; }

    // tokens[0] is the verb (ask/tell) — skip it.
    let rest = &tokens[1..];

    // Find the structural "about" token.
    let about_idx = rest.iter().position(|t| *t == "about")?;

    let name_fillers = ["the", "a", "an"];

    let npc_tokens: Vec<&str> = rest[..about_idx]
        .iter()
        .filter(|t| !name_fillers.contains(*t))
        .copied()
        .collect();

    let topic_tokens: Vec<&str> = rest[about_idx + 1..]
        .iter()
        .filter(|t| !name_fillers.contains(*t))
        .copied()
        .collect();

    if topic_tokens.is_empty() { return None; }

    let npc = if npc_tokens.is_empty() { None } else { Some(npc_tokens.join(" ")) };
    let topic = topic_tokens.join(" ");

    Some((npc, topic))
}

/// Parse raw input into a verb + optional noun.
/// Handles aliases and strips filler words.
pub fn parse(input: &str) -> Command {
    let tokens: Vec<&str> = input.split_whitespace().collect();

    if tokens.is_empty() {
        return Command { verb: String::new(), noun: None };
    }

    // Normalize verb aliases
    let verb = match tokens[0] {
        "n"                                         => "north",
        "s"                                         => "south",
        "e"                                         => "east",
        "w"                                         => "west",
        "u" | "climb"                               => "up",
        "d" | "descend"                             => "down",
        "l" | "look" | "examine" | "x"             => "look",
        "i" | "inv" | "inventory"                  => "inventory",
        "get" | "pick" | "grab"                    => "take",
        "read" | "peruse"                           => "read",
        "drop" | "discard" | "leave"               => "drop",
        "unlock" | "open"                           => "unlock",
        "quit" | "exit" | "q"                       => "quit",
        "h" | "help" | "?"                          => "help",
        "z"                                         => "wait",
        "g"                                         => "again",
        "smell" | "sniff"                           => "smell",
        "listen" | "hear"                           => "listen",
        "touch" | "feel"                            => "touch",
        "search" | "rummage"                        => "search",
        "push" | "shove"                            => "push",
        "pull" | "drag"                             => "pull",
        "turn" | "rotate"                           => "turn",
        "press"                                     => "press",
        "knock" | "rap"                             => "knock",
        "wear" | "don" | "put"                      => "wear",
        "remove" | "doff" | "unwear"               => "remove",
        other                                       => other,
    };

    // Everything after the verb is the noun, minus filler words
    let fillers = ["the", "a", "an", "at", "up", "down", "from", "on", "off", "in", "with"];
    let noun: Vec<&str> = tokens[1..]
        .iter()
        .filter(|w| !fillers.contains(w))
        .copied()
        .collect();

    let noun = if noun.is_empty() {
        None
    } else {
        Some(noun.join("_"))
    };

    Command {
        verb: verb.to_string(),
        noun,
    }
}
