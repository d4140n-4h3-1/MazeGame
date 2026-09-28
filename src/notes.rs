//! The notes and diary entries on the maze's computers, read once one is hacked: loaded from
//! [`NOTES`], whose `about` says what everything in it means, and shared out among the computers
//! afresh each maze by [`share_out`]. Entries of a group - a diary, a run of logs - stay together
//! on one computer, in order; the rest go wherever they fall.

use crate::layout::Rng;
use serde::Deserialize;

/// The file the entries are in.
pub const NOTES: &str = "data/notes.json";

/// One note or diary entry.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Entry {
    /// What it is called in the computer's listing, and what kind of entry it is.
    pub title: String,
    pub kind: String,
    /// When it was written, if it says.
    #[serde(default)]
    pub date: Option<String>,
    pub text: String,
    /// The entries it is kept together with, if any.
    #[serde(default)]
    pub group: Option<String>,
}

/// Every entry, as the file has them.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Notes {
    pub entries: Vec<Entry>,
}

impl Notes {
    /// The entries in the file at `path`.
    pub fn load(path: &str) -> Result<Self, String> {
        let text = crate::platform::read_to_string(path)?;
        serde_json::from_str(&text).map_err(|error| format!("{path}: {error}"))
    }
}

/// Shares `entries` out among `computers` computers: which entries each has, in order. Each group
/// goes whole onto one of them, and each entry of none onto one; always one of those with the
/// fewest entries so far, picked by `rng`, the groups first.
pub fn share_out(entries: &[Entry], computers: usize, rng: &mut Rng) -> Vec<Vec<usize>> {
    let mut shared = vec![Vec::new(); computers];
    if computers == 0 {
        return shared;
    }
    // The groups in the order they first come, then the entries of none.
    let mut groups: Vec<(&str, Vec<usize>)> = Vec::new();
    let mut loose = Vec::new();
    for (n, entry) in entries.iter().enumerate() {
        match &entry.group {
            Some(group) => match groups.iter_mut().find(|(name, _)| name == group) {
                Some((_, members)) => members.push(n),
                None => groups.push((group, vec![n])),
            },
            None => loose.push(vec![n]),
        }
    }
    for bundle in groups.into_iter().map(|(_, members)| members).chain(loose) {
        let fewest = shared.iter().map(Vec::len).min().unwrap_or(0);
        let emptiest: Vec<usize> = (0..computers).filter(|&c| shared[c].len() == fewest).collect();
        let computer = emptiest[rng.below(emptiest.len())];
        shared[computer].extend(bundle);
    }
    shared
}

/// `text` wrapped into lines at most `width` characters wide, breaking between words where it
/// can; each of its own lines kept, blank ones too.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word.to_string();
            loop {
                let (had, len) = (line.chars().count(), word.chars().count());
                let gap = usize::from(had > 0);
                if had + gap + len <= width {
                    if gap == 1 {
                        line.push(' ');
                    }
                    line.push_str(&word);
                    break;
                }
                if had > 0 {
                    lines.push(std::mem::take(&mut line));
                    continue;
                }
                // Too long for a line of its own: as much as fits, and the rest on the next.
                let cut = word.char_indices().nth(width).map_or(word.len(), |(i, _)| i);
                lines.push(word[..cut].to_string());
                word = word[cut..].to_string();
            }
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, group: Option<&str>) -> Entry {
        Entry {
            title: title.into(),
            kind: "note".into(),
            date: None,
            text: String::new(),
            group: group.map(Into::into),
        }
    }

    #[test]
    fn groups_stay_together_in_order_and_everyone_gets_a_share() {
        let entries = vec![
            entry("a1", Some("a")),
            entry("x", None),
            entry("b1", Some("b")),
            entry("a2", Some("a")),
            entry("y", None),
            entry("a3", Some("a")),
            entry("z", None),
        ];
        for seed in 1..50 {
            let shared = share_out(&entries, 4, &mut Rng::new(seed));
            let mut all: Vec<usize> = shared.iter().flatten().copied().collect();
            all.sort();
            assert_eq!(all, (0..entries.len()).collect::<Vec<_>>(), "each once");
            let holding = |n: usize| shared.iter().position(|c| c.contains(&n)).unwrap();
            assert!(holding(0) == holding(3) && holding(3) == holding(5), "{shared:?}");
            let a = &shared[holding(0)];
            assert_eq!(a.iter().filter(|&&n| [0, 3, 5].contains(&n)).count(), 3);
            assert!(a.windows(2).all(|w| w[0] < w[1]), "in order: {a:?}");
            assert!(shared.iter().all(|c| !c.is_empty()), "{shared:?}");
        }
    }

    #[test]
    fn the_shares_differ_from_maze_to_maze() {
        let entries: Vec<Entry> = (0..8).map(|n| entry(&n.to_string(), None)).collect();
        let shares: Vec<_> = (1..20).map(|s| share_out(&entries, 6, &mut Rng::new(s))).collect();
        assert!(shares.iter().any(|s| s != &shares[0]));
    }

    #[test]
    fn text_wraps_between_words_and_keeps_its_paragraphs() {
        let lines = wrap("one two three\n\nfour abcdefghijkl", 9);
        assert_eq!(lines, ["one two", "three", "", "four", "abcdefghi", "jkl"]);
    }

    #[test]
    fn the_notes_load_and_every_group_holds_together() {
        let notes = Notes::load(NOTES).unwrap();
        assert!(notes.entries.len() >= 6);
        for entry in &notes.entries {
            assert!(!entry.title.is_empty() && !entry.text.is_empty(), "{entry:?}");
        }
    }
}
