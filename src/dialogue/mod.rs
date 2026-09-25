//! Talking to the maze's inhabitants, as in Fallout 3: E to talk to a droid close by and in
//! front of you, and the camera closes in on its face. What it says runs along the bottom of the
//! screen, in its own System Latin (see `data/system_latin.md`) with what that means under it, and
//! under that the replies to pick from - with the mouse, W and S or the arrows and E, or the
//! number keys. Tab walks away.
//!
//! What each droid says is in `data/dialogue/droids.json`, whose `about` says what everything in
//! it means, and which can be rewritten without a rebuild. Each kind of droid has a
//! conversation of its own: lines, and the replies to each that lead on to other lines or end
//! it. A reply can be a skill check, `[Speech 40%]`, that goes one way if it succeeds and another
//! if it fails; a check is tried only once. A reply already given is shown dimmed.
//!
//! Each line has a mood, which colours the whole panel, and the droid's eyes: green as usual, blue
//! for success, yellow for a warning or a question, orange for agitation, red for hostility. A
//! line says its own, or takes one from how the check that led to it went: blue if it succeeded,
//! orange if not.
//!
//! A line can have the droid turn on the player once the conversation is over (see
//! [`Conversation::attacks`]): a sentry that sees through the player goes after them. A kind of
//! droid can also have [`Bark`]s, said out loud with nobody talking to it, as it hunts the
//! player: when it spots them, when it loses them, when it hears them, and when it gives up
//! looking.
//!
//! Every kind of droid can say how it takes having the pistol pointed at it ([`Threatened`]):
//! how long it will stand for it before warning the player, and what it does once it has warned
//! them twice - a sentry goes after them, anyone else sounds the alarm for the sentries.
//!
//! Lines can name what is true where the conversation happens, in braces: `{code}`, the droid's
//! code, and `{exit_far}` and `{exit_way}`, how far off the exit is and which way. Each is put in
//! System Latin where the droid says it, and in English where the meaning is given.
//!
//! [`screen`] draws it all.

pub mod screen;

use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// Where the droids' conversations are.
pub const SCRIPT: &str = "data/dialogue/droids.json";

/// A skill check on a reply: which skill, and the chance it succeeds, in percent.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Check {
    pub skill: String,
    pub chance: u32,
}

/// Something the player can say back.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Reply {
    pub say: String,
    /// The line it leads to; none ends the conversation.
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub check: Option<Check>,
    /// The line a failed check leads to instead.
    #[serde(default)]
    pub fail: Option<String>,
}

/// How a droid feels saying a line, which colours the panel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mood {
    /// Green.
    #[default]
    Normal,
    /// Blue.
    Success,
    /// Yellow: warning, or questioning.
    Warning,
    /// Orange: agitated, or a failed check.
    Agitated,
    /// Red.
    Hostile,
}

/// Something a droid says, and what the player can say back.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Line {
    /// In System Latin.
    pub says: String,
    /// What that means, in English.
    #[serde(default)]
    pub means: String,
    /// None at all leaves only walking away.
    #[serde(default)]
    pub replies: Vec<Reply>,
    /// None takes the mood from how the check that led here went, if one did.
    #[serde(default)]
    pub mood: Option<Mood>,
    /// Whether the droid goes after the player once the conversation is over.
    #[serde(default)]
    pub attacks: bool,
}

/// Something a droid says out loud by itself, with nobody talking to it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Bark {
    /// In System Latin.
    pub says: String,
    /// What that means, in English.
    #[serde(default)]
    pub means: String,
}

/// What a droid does once it has had the pistol pointed at it for too long.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provoked {
    /// It goes after the player.
    Attacks,
    /// It sounds the alarm, for the droids that go after the player to come and look.
    Alarm,
}

/// How a droid takes having the pistol pointed at it.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Threatened {
    /// How long it stands for it, in seconds, before it warns the player; as long again before
    /// its last warning; and as long again before it does something about it.
    pub patience: f32,
    pub then: Provoked,
}

/// A kind of droid, and how a conversation with one goes.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Character {
    /// What it is called, before its code.
    pub name: String,
    /// The line it opens with.
    pub start: String,
    pub lines: HashMap<String, Line>,
    /// What it says by itself, by when: as it hunts the player, `spotted`, `lost`, `heard`,
    /// `alarmed` or `gave_up`; with the pistol pointed at it, `warned`, `warned_again`,
    /// `provoked` or `calmed`.
    #[serde(default)]
    pub barks: HashMap<String, Bark>,
    /// How it takes having the pistol pointed at it; not at all, without.
    #[serde(default)]
    pub threatened: Option<Threatened>,
}

/// Everyone's conversations, as the file has them.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Script {
    pub characters: Vec<Character>,
}

impl Script {
    /// The conversations in the file at `path`, as long as every reply leads somewhere there is.
    pub fn load(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        let script: Self =
            serde_json::from_str(&text).map_err(|error| format!("{path}: {error}"))?;
        match script.problems().first() {
            Some(problem) => Err(format!("{path}: {problem}")),
            None => Ok(script),
        }
    }

    /// Whatever in the conversations leads nowhere.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for character in &self.characters {
            let name = &character.name;
            let lines = &character.lines;
            if !lines.contains_key(&character.start) {
                problems.push(format!("{name} starts with {}, which it does not have", character.start));
            }
            for (key, line) in lines {
                for reply in &line.replies {
                    let leads = reply.to.iter().chain(&reply.fail);
                    for to in leads.filter(|to| !lines.contains_key(*to)) {
                        problems.push(format!("{name}'s {key}: \"{}\" leads to {to}, which it does not have", reply.say));
                    }
                    if reply.check.is_some() && reply.fail.is_none() {
                        problems.push(format!("{name}'s {key}: \"{}\" is a check with no fail", reply.say));
                    }
                }
            }
        }
        problems
    }
}

/// What is true where a conversation happens, for lines to name: each `{key}`, in System Latin
/// and in English.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts(Vec<(&'static str, String, String)>);

impl Facts {
    pub fn with(mut self, key: &'static str, latin: impl Into<String>, english: impl Into<String>) -> Self {
        self.0.push((key, latin.into(), english.into()));
        self
    }

    /// `text` with every `{key}` it names filled in, in System Latin or in English.
    fn fill(&self, text: &str, latin: bool) -> String {
        self.0.iter().fold(text.to_string(), |text, (key, l, e)| {
            text.replace(&format!("{{{key}}}"), if latin { l } else { e })
        })
    }
}

/// A number as System Latin reads out a code: digit by digit (see section XXX of
/// `data/system_latin.md`).
pub fn digits(number: u32) -> String {
    const DIGITS: [&str; 10] = [
        "zero", "unum", "duo", "tria", "quattuor", "quinque", "sex", "septem", "octo", "novem",
    ];
    number
        .to_string()
        .bytes()
        .map(|digit| DIGITS[(digit - b'0') as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

/// What the player can say back, as shown.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    /// With its check, if it has one, in front.
    pub label: String,
    /// Whether it has been said before, and is dimmed.
    pub said: bool,
}

/// What is on screen for the line the conversation is at.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub says: String,
    pub means: String,
    /// How the last check went, if the last reply was one.
    pub note: Option<String>,
    pub choices: Vec<Choice>,
    pub mood: Mood,
}

/// Where a conversation with one droid has got to.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversation {
    character: usize,
    line: String,
    /// The replies said so far, by the line they were said to and which of its replies they were.
    said: HashSet<(String, usize)>,
    /// How the last check went, if the last reply was one: what to say about it, and whether it
    /// succeeded.
    note: Option<(String, bool)>,
}

impl Conversation {
    /// A conversation with a droid that is `character` of `script`, from its opening line.
    pub fn new(script: &Script, character: usize) -> Option<Self> {
        let start = script.characters.get(character)?.start.clone();
        Some(Self {
            character,
            line: start,
            said: HashSet::new(),
            note: None,
        })
    }

    fn line<'a>(&self, script: &'a Script) -> Option<&'a Line> {
        script.characters.get(self.character)?.lines.get(&self.line)
    }

    /// The replies there are to pick from, as indices into the line's replies: all but the checks
    /// already tried. None for the line's end, when it has no replies.
    fn offered(&self, line: &Line) -> Vec<Option<usize>> {
        let offered: Vec<Option<usize>> = (0..line.replies.len())
            .filter(|&i| {
                line.replies[i].check.is_none() || !self.said.contains(&(self.line.clone(), i))
            })
            .map(Some)
            .collect();
        if offered.is_empty() {
            vec![None]
        } else {
            offered
        }
    }

    /// What there is to show, with `facts` filled in.
    pub fn view(&self, script: &Script, facts: &Facts) -> View {
        let Some(line) = self.line(script) else {
            return View {
                says: String::new(),
                means: String::new(),
                note: None,
                choices: vec![leave()],
                mood: Mood::Normal,
            };
        };
        let choices = self
            .offered(line)
            .into_iter()
            .map(|index| {
                let Some(index) = index else {
                    return leave();
                };
                let reply = &line.replies[index];
                let say = facts.fill(&reply.say, false);
                Choice {
                    label: match &reply.check {
                        Some(check) => format!("[{} {}%] {say}", check.skill, check.chance),
                        None => say,
                    },
                    said: self.said.contains(&(self.line.clone(), index)),
                }
            })
            .collect();
        let mood = line.mood.unwrap_or(match self.note {
            Some((_, true)) => Mood::Success,
            Some((_, false)) => Mood::Agitated,
            None => Mood::Normal,
        });
        View {
            says: facts.fill(&line.says, true),
            means: facts.fill(&line.means, false),
            note: self.note.as_ref().map(|(note, _)| note.clone()),
            choices,
            mood,
        }
    }

    /// Whether the line the conversation is at has the droid go after the player once it is
    /// over.
    pub fn attacks(&self, script: &Script) -> bool {
        self.line(script).is_some_and(|line| line.attacks)
    }

    /// Says the `choice`th of the replies on offer, rolling `roll` - from 0 to 99 - for its check
    /// if it has one. False once the conversation is over.
    pub fn choose(&mut self, script: &Script, choice: usize, roll: u32) -> bool {
        let Some(line) = self.line(script) else {
            return false;
        };
        let Some(&Some(index)) = self.offered(line).get(choice) else {
            // Past the end of what is on offer is nothing; walking away from a line with no
            // replies ends it.
            return choice >= self.offered(line).len();
        };
        let reply = &line.replies[index];
        self.said.insert((self.line.clone(), index));
        self.note = None;
        let next = match &reply.check {
            Some(check) => {
                let passed = roll < check.chance;
                self.note = Some((
                    format!(
                        "[{} {}%] {}",
                        check.skill,
                        check.chance,
                        if passed { "Succeeded" } else { "Failed" }
                    ),
                    passed,
                ));
                if passed {
                    &reply.to
                } else {
                    &reply.fail
                }
            }
            None => &reply.to,
        };
        match next {
            Some(next) => {
                self.line = next.clone();
                true
            }
            None => false,
        }
    }
}

/// Walking away, for a line nobody can reply to.
fn leave() -> Choice {
    Choice {
        label: "[Leave]".to_string(),
        said: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script() -> Script {
        serde_json::from_str(
            r#"{ "characters": [ { "name": "Explorator", "start": "hello", "lines": {
                "hello": { "says": "Explorator codex {code}.", "means": "Scout {code}.",
                    "replies": [
                        { "say": "Where is the exit?", "check": { "skill": "Speech", "chance": 50 },
                          "to": "exit", "fail": "no" },
                        { "say": "Tell me again.", "to": "hello" },
                        { "say": "Goodbye." } ] },
                "exit": { "says": "Progreda {exit_way}.", "means": "Go {exit_way}." },
                "no": { "says": "Negativum.", "means": "No.",
                    "replies": [ { "say": "Back.", "to": "hello" } ] },
                "cross": { "says": "Sta.", "means": "Stop.", "mood": "hostile",
                    "attacks": true } } } ] }"#,
        )
        .unwrap()
    }

    fn facts() -> Facts {
        Facts::default()
            .with("code", "quattuor septem", "47")
            .with("exit_way", "ad sinistrum", "to your left")
    }

    #[test]
    fn it_opens_with_the_first_line_and_fills_in_what_is_known() {
        let script = script();
        let view = Conversation::new(&script, 0).unwrap().view(&script, &facts());
        assert_eq!(view.says, "Explorator codex quattuor septem.");
        assert_eq!(view.means, "Scout 47.");
        let labels: Vec<_> = view.choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["[Speech 50%] Where is the exit?", "Tell me again.", "Goodbye."]);
    }

    #[test]
    fn a_check_goes_one_way_or_the_other_and_is_tried_only_once() {
        let script = script();
        let mut talk = Conversation::new(&script, 0).unwrap();
        assert!(talk.choose(&script, 0, 50), "a roll of 50 fails a 50% check");
        let view = talk.view(&script, &facts());
        assert_eq!(view.says, "Negativum.");
        assert_eq!(view.note.as_deref(), Some("[Speech 50%] Failed"));
        assert!(talk.choose(&script, 0, 0));
        let view = talk.view(&script, &facts());
        assert_eq!(view.note, None);
        assert_eq!(view.choices.len(), 2, "the check is gone");

        let mut talk = Conversation::new(&script, 0).unwrap();
        assert!(talk.choose(&script, 0, 49));
        assert_eq!(talk.view(&script, &facts()).says, "Progreda ad sinistrum.");
    }

    #[test]
    fn a_line_has_its_own_mood_or_the_one_its_check_gives_it() {
        let script = script();
        let mut talk = Conversation::new(&script, 0).unwrap();
        assert_eq!(talk.view(&script, &facts()).mood, Mood::Normal);
        talk.choose(&script, 0, 99);
        assert_eq!(talk.view(&script, &facts()).mood, Mood::Agitated, "failed");
        talk.choose(&script, 0, 0);
        assert_eq!(talk.view(&script, &facts()).mood, Mood::Normal, "no check this time");
        let mut talk = Conversation::new(&script, 0).unwrap();
        talk.choose(&script, 0, 0);
        assert_eq!(talk.view(&script, &facts()).mood, Mood::Success, "succeeded");
        talk.line = "cross".into();
        assert_eq!(talk.view(&script, &facts()).mood, Mood::Hostile, "its own");
    }

    #[test]
    fn only_a_line_that_says_so_ends_in_an_attack() {
        let script = script();
        let mut talk = Conversation::new(&script, 0).unwrap();
        assert!(!talk.attacks(&script));
        talk.line = "cross".into();
        assert!(talk.attacks(&script));
    }

    #[test]
    fn what_has_been_said_is_dimmed() {
        let script = script();
        let mut talk = Conversation::new(&script, 0).unwrap();
        talk.choose(&script, 1, 0);
        let view = talk.view(&script, &facts());
        assert!(view.choices[1].said && !view.choices[2].said);
    }

    #[test]
    fn goodbye_and_a_line_with_no_replies_end_it() {
        let script = script();
        let mut talk = Conversation::new(&script, 0).unwrap();
        assert!(!talk.choose(&script, 2, 0), "goodbye");
        let mut talk = Conversation::new(&script, 0).unwrap();
        talk.choose(&script, 0, 0);
        let view = talk.view(&script, &facts());
        assert_eq!(view.choices, [leave()]);
        assert!(!talk.choose(&script, 0, 0), "leaving");
    }

    #[test]
    fn a_reply_that_leads_nowhere_is_found() {
        let mut script = script();
        script.characters[0].lines.get_mut("no").unwrap().replies[0].to = Some("gone".into());
        assert_eq!(script.problems().len(), 1);
    }

    #[test]
    fn codes_are_read_digit_by_digit() {
        assert_eq!(digits(47), "quattuor septem");
        assert_eq!(digits(90), "novem zero");
    }

    #[test]
    fn the_droids_conversations_load() {
        let script = Script::load(SCRIPT).unwrap();
        assert!(!script.characters.is_empty());
    }
}
