//! Talking to the maze's inhabitants, as in Fallout 3: E to talk to a droid close by and in
//! front of you, and the camera closes in on its face. What it says runs along the bottom of the
//! screen, in its own System Latin (see `data/system_latin/system_latin_systemized.md`) with
//! what that means under it, and under that the replies to pick from - with the mouse, W and S or
//! the arrows and E, or the number keys. Tab walks away.
//!
//! What each droid says is in `data/dialogue/droids.json`, whose `about` says what everything in
//! it means, and which can be rewritten without a rebuild. Each kind of droid has a
//! conversation of its own: lines, and the replies to each that lead on to other lines or end
//! it. A reply can be a skill check, `[Speech 40%]`, that goes one way if it succeeds and another
//! if it fails; a check is tried only once. A reply already given is shown dimmed.
//!
//! A check can also be tried with a bribe (see [`Bribe`]), offered under it as a reply of its own,
//! `[Speech 75%] [40.00 CR]`: the same check, more likely to succeed, for credits paid whether it
//! does or not. It is one try with the check: trying either takes both away. Without the credits
//! it is shown dimmed, and cannot be picked. Only droids that know something worth paying for
//! are offered bribes - not maintenance units, which do not know where the exit is - and drones
//! cannot be talked to at all.
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

use crate::credits::Credits;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// Where the droids' conversations are.
pub const SCRIPT: &str = "data/dialogue/droids.json";
/// What the droids say in capture the flag, where they only call out, and are not talked to.
pub const CTF_SCRIPT: &str = "data/dialogue/ctf.json";

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
    /// The check tried with a bribe instead, if the droid takes one.
    #[serde(default)]
    pub bribe: Option<Bribe>,
}

/// A check tried with credits: what the player says instead, how many credits it costs, and the
/// chance the check succeeds with them, in percent - higher than without. It goes where the
/// check would, and the credits are paid either way.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Bribe {
    pub say: String,
    pub credits: Credits,
    pub chance: u32,
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
    /// The line it opens with, and its lines; none, for one that is not talked to.
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub lines: HashMap<String, Line>,
    /// What it says by itself, by when: as it hunts the player, `spotted`, `lost`, `heard`,
    /// `alarmed` or `gave_up`; with the pistol pointed at it, `warned`, `warned_again`,
    /// `provoked` or `calmed`.
    #[serde(default)]
    pub barks: HashMap<String, Bark>,
    /// How it takes having the pistol pointed at it; not at all, without.
    #[serde(default)]
    pub threatened: Option<Threatened>,
    /// The model it is made from; the player's droid's, without.
    #[serde(default)]
    pub model: Option<String>,
    /// Whether it changes into the hostile droid's colours while it is after the player. Sentries
    /// keep their own; the colours are for another kind of droid.
    #[serde(default)]
    pub hostile_colours: bool,
}

/// Everyone's conversations, as the file has them.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Script {
    pub characters: Vec<Character>,
}

impl Script {
    /// The conversations in the file at `path`, as long as every reply leads somewhere there is.
    pub fn load(path: &str) -> Result<Self, String> {
        let text = crate::platform::read_to_string(path)?;
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
            if !lines.is_empty() && !lines.contains_key(&character.start) {
                problems.push(format!(
                    "{name} starts with {}, which it does not have",
                    character.start
                ));
            }
            for (key, line) in lines {
                for reply in &line.replies {
                    let leads = reply.to.iter().chain(&reply.fail);
                    for to in leads.filter(|to| !lines.contains_key(*to)) {
                        problems.push(format!(
                            "{name}'s {key}: \"{}\" leads to {to}, which it does not have",
                            reply.say
                        ));
                    }
                    if reply.check.is_some() && reply.fail.is_none() {
                        problems.push(format!(
                            "{name}'s {key}: \"{}\" is a check with no fail",
                            reply.say
                        ));
                    }
                    match (&reply.check, &reply.bribe) {
                        (None, Some(_)) => problems.push(format!(
                            "{name}'s {key}: \"{}\" has a bribe but no check",
                            reply.say
                        )),
                        (Some(check), Some(bribe)) if bribe.chance <= check.chance => {
                            problems.push(format!(
                                "{name}'s {key}: \"{}\" has a bribe no likelier to work",
                                reply.say
                            ))
                        }
                        _ => (),
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
    pub fn with(
        mut self,
        key: &'static str,
        latin: impl Into<String>,
        english: impl Into<String>,
    ) -> Self {
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
/// `data/system_latin/system_latin_systemized.md`).
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

    /// The replies there are to pick from, as indices into the line's replies, each with whether
    /// it is the reply's bribe: all but the checks already tried, each check's bribe after it.
    /// None for the line's end, when it has no replies.
    fn offered(&self, line: &Line) -> Vec<Option<(usize, bool)>> {
        let offered: Vec<Option<(usize, bool)>> = (0..line.replies.len())
            .filter(|&i| {
                line.replies[i].check.is_none() || !self.said.contains(&(self.line.clone(), i))
            })
            .flat_map(|i| {
                let bribe = line.replies[i].check.is_some() && line.replies[i].bribe.is_some();
                std::iter::once(Some((i, false))).chain(bribe.then_some(Some((i, true))))
            })
            .collect();
        if offered.is_empty() {
            vec![None]
        } else {
            offered
        }
    }

    /// What there is to show, with `facts` filled in, for a player with `credits`.
    pub fn view(&self, script: &Script, facts: &Facts, credits: Credits) -> View {
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
            .map(|offered| {
                let Some((index, bribed)) = offered else {
                    return leave();
                };
                let reply = &line.replies[index];
                match (&reply.check, reply.bribe.as_ref().filter(|_| bribed)) {
                    // Dimmed while the player cannot pay.
                    (Some(check), Some(bribe)) => Choice {
                        label: format!(
                            "[{} {}%] [{}] {}",
                            check.skill,
                            bribe.chance,
                            bribe.credits,
                            facts.fill(&bribe.say, false)
                        ),
                        said: credits < bribe.credits,
                    },
                    (check, _) => {
                        let say = facts.fill(&reply.say, false);
                        Choice {
                            label: match check {
                                Some(check) => format!("[{} {}%] {say}", check.skill, check.chance),
                                None => say,
                            },
                            said: self.said.contains(&(self.line.clone(), index)),
                        }
                    }
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

    /// What the `choice`th of the replies on offer costs, if it is a bribe.
    pub fn price(&self, script: &Script, choice: usize) -> Option<Credits> {
        let line = self.line(script)?;
        let (index, true) = (*self.offered(line).get(choice)?)? else {
            return None;
        };
        Some(line.replies[index].bribe.as_ref()?.credits)
    }

    /// Says the `choice`th of the replies on offer, rolling `roll` - from 0 to 99 - for its check
    /// if it has one, and paying for its bribe from `credits` if it is one; a bribe the player
    /// cannot pay for is not said. False once the conversation is over.
    pub fn choose(&mut self, script: &Script, choice: usize, roll: u32, credits: &mut Credits) -> bool {
        let Some(line) = self.line(script) else {
            return false;
        };
        let Some(&Some((index, bribed))) = self.offered(line).get(choice) else {
            // Past the end of what is on offer is nothing; walking away from a line with no
            // replies ends it.
            return choice >= self.offered(line).len();
        };
        let reply = &line.replies[index];
        let bribe = reply.bribe.as_ref().filter(|_| bribed);
        if bribe.is_some_and(|bribe| !credits.spend(bribe.credits)) {
            return true;
        }
        self.said.insert((self.line.clone(), index));
        self.note = None;
        let next = match &reply.check {
            Some(check) => {
                let chance = bribe.map_or(check.chance, |bribe| bribe.chance);
                let passed = roll < chance;
                let paid = bribe.map_or(String::new(), |bribe| format!(", {} paid", bribe.credits));
                self.note = Some((
                    format!(
                        "[{} {}%] {}{paid}",
                        check.skill,
                        chance,
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
        Facts::default().with("code", "quattuor septem", "47").with(
            "exit_way",
            "ad sinistrum",
            "to your left",
        )
    }

    #[test]
    fn it_opens_with_the_first_line_and_fills_in_what_is_known() {
        let script = script();
        let view = Conversation::new(&script, 0)
            .unwrap()
            .view(&script, &facts(), Credits::default());
        assert_eq!(view.says, "Explorator codex quattuor septem.");
        assert_eq!(view.means, "Scout 47.");
        let labels: Vec<_> = view.choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "[Speech 50%] Where is the exit?",
                "Tell me again.",
                "Goodbye."
            ]
        );
    }

    #[test]
    fn a_check_goes_one_way_or_the_other_and_is_tried_only_once() {
        let script = script();
        let mut talk = Conversation::new(&script, 0).unwrap();
        assert!(
            talk.choose(&script, 0, 50, &mut Credits::default()),
            "a roll of 50 fails a 50% check"
        );
        let view = talk.view(&script, &facts(), Credits::default());
        assert_eq!(view.says, "Negativum.");
        assert_eq!(view.note.as_deref(), Some("[Speech 50%] Failed"));
        assert!(talk.choose(&script, 0, 0, &mut Credits::default()));
        let view = talk.view(&script, &facts(), Credits::default());
        assert_eq!(view.note, None);
        assert_eq!(view.choices.len(), 2, "the check is gone");

        let mut talk = Conversation::new(&script, 0).unwrap();
        assert!(talk.choose(&script, 0, 49, &mut Credits::default()));
        assert_eq!(talk.view(&script, &facts(), Credits::default()).says, "Progreda ad sinistrum.");
    }

    #[test]
    fn a_line_has_its_own_mood_or_the_one_its_check_gives_it() {
        let script = script();
        let mut talk = Conversation::new(&script, 0).unwrap();
        assert_eq!(talk.view(&script, &facts(), Credits::default()).mood, Mood::Normal);
        talk.choose(&script, 0, 99, &mut Credits::default());
        assert_eq!(talk.view(&script, &facts(), Credits::default()).mood, Mood::Agitated, "failed");
        talk.choose(&script, 0, 0, &mut Credits::default());
        assert_eq!(
            talk.view(&script, &facts(), Credits::default()).mood,
            Mood::Normal,
            "no check this time"
        );
        let mut talk = Conversation::new(&script, 0).unwrap();
        talk.choose(&script, 0, 0, &mut Credits::default());
        assert_eq!(
            talk.view(&script, &facts(), Credits::default()).mood,
            Mood::Success,
            "succeeded"
        );
        talk.line = "cross".into();
        assert_eq!(talk.view(&script, &facts(), Credits::default()).mood, Mood::Hostile, "its own");
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
        talk.choose(&script, 1, 0, &mut Credits::default());
        let view = talk.view(&script, &facts(), Credits::default());
        assert!(view.choices[1].said && !view.choices[2].said);
    }

    #[test]
    fn goodbye_and_a_line_with_no_replies_end_it() {
        let script = script();
        let mut talk = Conversation::new(&script, 0).unwrap();
        assert!(!talk.choose(&script, 2, 0, &mut Credits::default()), "goodbye");
        let mut talk = Conversation::new(&script, 0).unwrap();
        talk.choose(&script, 0, 0, &mut Credits::default());
        let view = talk.view(&script, &facts(), Credits::default());
        assert_eq!(view.choices, [leave()]);
        assert!(!talk.choose(&script, 0, 0, &mut Credits::default()), "leaving");
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

    #[test]
    fn capture_the_flags_droids_only_call_out_each_side_in_a_voice_of_its_own() {
        let script = Script::load(CTF_SCRIPT).unwrap();
        let voices = crate::formants::speech::Voices::load(crate::formants::speech::VOICES).unwrap();
        let [red, blue] = &script.characters[..] else {
            panic!("a kind of droid for each side");
        };
        let mut pitches = Vec::new();
        for character in [red, blue] {
            assert!(character.lines.is_empty(), "{} is not talked to", character.name);
            assert!(character.barks.contains_key("engaged"), "{} calls out", character.name);
            let voice = voices.voices.get(&character.name).expect("a voice of its own");
            pitches.push(voice.pitch);
        }
        assert!(pitches[0] > 2.0 * pitches[1], "told apart by ear");
    }

    fn bribable() -> Script {
        serde_json::from_str(
            r#"{ "characters": [ { "name": "Defendator", "start": "hello", "lines": {
                "hello": { "says": "Sta.", "means": "Halt.",
                    "replies": [
                        { "say": "I'm maintenance.", "check": { "skill": "Speech", "chance": 40 },
                          "bribe": { "say": "I'm maintenance. Here.", "credits": 40, "chance": 75 },
                          "to": "cleared", "fail": "suspect" },
                        { "say": "Goodbye." } ] },
                "cleared": { "says": "Progreda.", "means": "Proceed." },
                "suspect": { "says": "Suspecto tu.", "means": "I suspect you.",
                    "replies": [ { "say": "Back.", "to": "hello" } ] } } } ] }"#,
        )
        .unwrap()
    }

    #[test]
    fn a_bribe_is_offered_under_its_check_and_dimmed_without_the_credits() {
        let script = bribable();
        let talk = Conversation::new(&script, 0).unwrap();
        let view = talk.view(&script, &facts(), Credits::new(39, 99));
        let labels: Vec<_> = view.choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "[Speech 40%] I'm maintenance.",
                "[Speech 75%] [40.00 CR] I'm maintenance. Here.",
                "Goodbye."
            ]
        );
        assert!(view.choices[1].said, "dimmed: too poor");
        assert!(!talk.view(&script, &facts(), Credits::new(40, 0)).choices[1].said);
        assert_eq!(talk.price(&script, 1), Some(Credits::new(40, 0)));
        assert_eq!(talk.price(&script, 0), None);
    }

    #[test]
    fn a_bribe_is_a_likelier_check_paid_for_either_way_and_tried_only_once() {
        let script = bribable();
        // Too poor: nothing happens, and nothing is paid.
        let mut talk = Conversation::new(&script, 0).unwrap();
        let mut poor = Credits::new(10, 0);
        assert!(talk.choose(&script, 1, 0, &mut poor));
        assert_eq!(poor, Credits::new(10, 0));
        assert_eq!(talk.view(&script, &facts(), poor).says, "Sta.");
        // A roll the check alone would fail, and the bribe passes.
        let mut wallet = Credits::new(50, 25);
        assert!(talk.choose(&script, 1, 60, &mut wallet));
        assert_eq!(wallet, Credits::new(10, 25));
        let view = talk.view(&script, &facts(), wallet);
        assert_eq!(view.says, "Progreda.");
        assert_eq!(view.note.as_deref(), Some("[Speech 75%] Succeeded, 40.00 CR paid"));
        // Failing, it is paid all the same; and the check and its bribe are both gone after.
        let mut talk = Conversation::new(&script, 0).unwrap();
        let mut wallet = Credits::new(40, 0);
        assert!(talk.choose(&script, 1, 75, &mut wallet));
        assert_eq!(wallet, Credits::default());
        assert_eq!(talk.view(&script, &facts(), wallet).says, "Suspecto tu.");
        talk.choose(&script, 0, 0, &mut wallet);
        let labels: Vec<_> = talk
            .view(&script, &facts(), wallet)
            .choices
            .into_iter()
            .map(|c| c.label)
            .collect();
        assert_eq!(labels, ["Goodbye."]);
    }

    #[test]
    fn a_bribe_needs_a_check_and_better_odds_than_it() {
        let mut script = bribable();
        let reply = &mut script.characters[0].lines.get_mut("hello").unwrap().replies[0];
        reply.bribe.as_mut().unwrap().chance = 40;
        assert_eq!(script.problems().len(), 1, "no likelier");
        let reply = &mut script.characters[0].lines.get_mut("hello").unwrap().replies[0];
        reply.check = None;
        reply.fail = None;
        assert_eq!(script.problems().len(), 1, "no check");
    }
}
