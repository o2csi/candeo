//! Rules, and the resolver deciding what they ask of a device at an instant.
//!
//! **Pure**: no clock, no device, no disk. Everything that changes the answer —
//! the instant, the rules, the pause, what someone dismissed or tried — comes in
//! as an argument, so every case below is a unit test rather than an evening in
//! front of a keyboard waiting for the hour (`docs/design/inputs-and-automations.md`
//! §3.4).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::DeviceRef;

/// Milliseconds in a day, the unit of every time of day here.
const DAY_MS: i64 = 86_400_000;

/// A rule, as `settings.json` holds it (§3.2).
///
/// > **Every** *3600 seconds* · **on** *DeathStalker V2 Pro* · **show** *Clock* ·
/// > **for** *10 seconds*
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Off unless said: a rule someone has not switched on does nothing, and the
    /// examples the interface offers start that way.
    #[serde(default)]
    pub enabled: bool,
    pub devices: Vec<DeviceRef>,
    pub when: Trigger,
    pub show: Show,
    #[serde(rename = "for", default)]
    pub lasts: Lasts,
}

/// When a rule applies. One kind for now; `signal` and `idle` come with their
/// own pull requests, as further variants of this tag.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Trigger {
    Schedule {
        /// Seconds between two occurrences, 1 or more.
        every: u32,
        /// Occurrences fall on the local clock rather than counting from when
        /// the rule was switched on. Absent means [`default_aligned`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aligned: Option<bool>,
        /// Only between two times; outside, the rule sleeps.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        between: Option<Window>,
    },
}

/// What a rule shows: an effect id, as `activeEffects` names them, and its own
/// settings — never the ones saved for that effect on the device, since the
/// clock on the hour and the clock applied by hand need not look alike.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Show {
    pub effect: String,
    #[serde(default)]
    pub params: serde_json::Map<String, serde_json::Value>,
}

/// How long an occurrence lasts.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lasts {
    pub seconds: u32,
}

impl Default for Lasts {
    fn default() -> Self {
        Self { seconds: 10 }
    }
}

/// A span of the day, `from` included and `to` excluded. `22:00`–`07:00` wraps
/// past midnight; `from` equal to `to` is the whole day.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub from: TimeOfDay,
    pub to: TimeOfDay,
}

impl Window {
    fn contains(self, day_ms: i64) -> bool {
        let (from, to) = (self.from.ms(), self.to.ms());
        match from.cmp(&to) {
            std::cmp::Ordering::Equal => true,
            std::cmp::Ordering::Less => from <= day_ms && day_ms < to,
            std::cmp::Ordering::Greater => day_ms >= from || day_ms < to,
        }
    }
}

/// A time of day to the minute, `HH:MM` in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeOfDay {
    minutes: u16,
}

impl TimeOfDay {
    pub fn parse(text: &str) -> Result<Self, String> {
        let invalid = || format!("\"{text}\" is not a time of day, as HH:MM");
        let (h, m) = text.split_once(':').ok_or_else(invalid)?;
        if h.len() != 2 || m.len() != 2 {
            return Err(invalid());
        }
        let (h, m): (u16, u16) = (
            h.parse().map_err(|_| invalid())?,
            m.parse().map_err(|_| invalid())?,
        );
        if h > 23 || m > 59 {
            return Err(invalid());
        }
        Ok(Self {
            minutes: h * 60 + m,
        })
    }

    fn ms(self) -> i64 {
        i64::from(self.minutes) * 60_000
    }
}

impl Serialize for TimeOfDay {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!(
            "{:02}:{:02}",
            self.minutes / 60,
            self.minutes % 60
        ))
    }
}

impl<'de> Deserialize<'de> for TimeOfDay {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        TimeOfDay::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// Whether a schedule of `every` seconds falls on the clock when it does not say.
///
/// On when it divides a day: every hour, every quarter of an hour, lands on the
/// same times each day. Every 7 seconds does not, and counting from when the
/// rule was switched on is then the only reading that means something.
pub fn default_aligned(every: u32) -> bool {
    every > 0 && 86_400 % every == 0
}

impl Rule {
    /// What makes a rule unusable, in English, for the log and the interface.
    ///
    /// A broken rule stays in the file and does nothing (§3.4): dropping it would
    /// lose what someone wrote over a typo.
    pub fn problem(&self) -> Option<String> {
        let Trigger::Schedule { every, .. } = &self.when;
        if *every == 0 {
            return Some("a schedule runs every 1 second or more".into());
        }
        if self.lasts.seconds == 0 {
            return Some("an occurrence lasts 1 second or more".into());
        }
        if self.show.effect.is_empty() {
            return Some("the rule shows no effect".into());
        }
        if self.devices.is_empty() {
            return Some("the rule targets no device".into());
        }
        None
    }
}

/// Reads the rules of `settings.json` one by one.
///
/// They are kept raw in the file: one malformed rule — edited by hand, written
/// by a later version — must not make the whole file unreadable, since that
/// would leave the application silent at startup. Each comes back with its own
/// verdict instead.
pub fn parse(raw: &[serde_json::Value]) -> Vec<Result<Rule, String>> {
    raw.iter()
        .map(|value| {
            let rule: Rule = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            match rule.problem() {
                Some(problem) => Err(problem),
                None => Ok(rule),
            }
        })
        .collect()
}

/// An instant, read on the two clocks a schedule needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moment {
    /// Milliseconds since the epoch: how long occurrences last, and which one
    /// is which.
    pub epoch_ms: i64,
    /// Milliseconds since local midnight: where the hands of the clock stand,
    /// which is what "on the hour" and "between 22:00 and 07:00" mean.
    pub day_ms: i64,
}

/// A rule interrupting a device: what the resolver answers.
#[derive(Clone, Debug, PartialEq)]
pub struct Interruption {
    pub rule: String,
    pub effect: String,
    pub params: serde_json::Map<String, serde_json::Value>,
    /// When this occurrence started, in epoch milliseconds. It names the
    /// occurrence: dismissing it leaves the next one alone.
    pub since: i64,
    /// When it ends, in epoch milliseconds; `None` for a rule that never stops,
    /// every second for a second with no window.
    pub until: Option<i64>,
}

/// Everything, besides the device, the answer depends on.
pub struct Context<'a> {
    pub now: Moment,
    pub rules: &'a [Rule],
    /// Pause automations: no schedule applies. A rule tried by hand still does.
    pub paused: bool,
    /// When each rule was first seen switched on, epoch milliseconds: the
    /// origin of a schedule not aligned on the clock.
    pub anchors: &'a HashMap<String, i64>,
    /// Occurrences someone ended — Resume, or an effect applied by hand — as
    /// `(device, rule, since)`. The rule comes back at its next occurrence.
    pub dismissed: &'a HashSet<(DeviceRef, String, i64)>,
    /// Rules tried by hand, and when: each runs once, for its duration, switched
    /// on or not, paused or not — the gesture wins.
    pub tried: &'a HashMap<String, i64>,
}

/// What should interrupt `device` now, if anything (§3.1).
///
/// The first answer wins: a rule tried by hand, then the scheduled rules in
/// their order, which is their priority. A dismissed occurrence is passed over,
/// and the next rule gets its turn.
pub fn resolve(context: &Context, device: DeviceRef) -> Option<Interruption> {
    let targets = |rule: &&Rule| rule.devices.contains(&device) && rule.problem().is_none();

    let tried = context.rules.iter().filter(targets).find_map(|rule| {
        let since = *context.tried.get(&rule.id)?;
        let until = since + i64::from(rule.lasts.seconds) * 1000;
        (since <= context.now.epoch_ms && context.now.epoch_ms < until)
            .then(|| interruption(rule, since, Some(until)))
    });

    tried
        .into_iter()
        .chain(
            context
                .rules
                .iter()
                .filter(|rule| !context.paused && rule.enabled)
                .filter(targets)
                .filter_map(|rule| {
                    let anchor = context.anchors.get(&rule.id).copied();
                    let (since, until) = occurrence(rule, context.now, anchor)?;
                    Some(interruption(rule, since, until))
                }),
        )
        .find(|found| {
            !context
                .dismissed
                .contains(&(device, found.rule.clone(), found.since))
        })
}

fn interruption(rule: &Rule, since: i64, until: Option<i64>) -> Interruption {
    Interruption {
        rule: rule.id.clone(),
        effect: rule.show.effect.clone(),
        params: rule.show.params.clone(),
        since,
        until,
    }
}

/// The occurrence of a scheduled rule under way at `now`, as `(since, until)`.
///
/// `anchor` is when the rule was switched on, for a schedule not aligned on the
/// clock; without it, such a rule has not started.
fn occurrence(rule: &Rule, now: Moment, anchor: Option<i64>) -> Option<(i64, Option<i64>)> {
    let Trigger::Schedule {
        every,
        aligned,
        between,
    } = &rule.when;
    let period = i64::from(*every) * 1000;
    let lasts = i64::from(rule.lasts.seconds) * 1000;
    let aligned = aligned.unwrap_or_else(|| default_aligned(*every));

    let window = between.filter(|w| w.from != w.to);
    if let Some(window) = window {
        if !window.contains(now.day_ms) {
            return None;
        }
    }
    // Where the window opened and where it closes, around `now`.
    let window_edges = window.map(|w| {
        let opened = now.epoch_ms - (now.day_ms - w.from.ms()).rem_euclid(DAY_MS);
        let closes = now.epoch_ms + (w.to.ms() - now.day_ms).rem_euclid(DAY_MS);
        (opened, closes)
    });
    let midnight = now.epoch_ms - now.day_ms;

    // An occurrence lasting as long as the period, or longer, reaches the next
    // one before it ends: the interruption goes on, and is one occurrence — the
    // window's, or the day's, or since the rule was switched on.
    if lasts >= period {
        let since = match (window_edges, aligned) {
            (Some((opened, _)), _) => opened,
            (None, true) => midnight,
            (None, false) => anchor?,
        };
        return Some((since, window_edges.map(|(_, closes)| closes)));
    }

    let phase = if aligned {
        now.day_ms.rem_euclid(period)
    } else {
        let elapsed = now.epoch_ms - anchor?;
        if elapsed < 0 {
            return None;
        }
        elapsed % period
    };
    if phase >= lasts {
        return None;
    }
    let since = now.epoch_ms - phase;
    let ends = since + lasts;
    let until = match window_edges {
        Some((_, closes)) => ends.min(closes),
        None => ends,
    };
    Some((since, Some(until)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYBOARD: DeviceRef = DeviceRef {
        vid: 0x1532,
        pid: 0x0292,
    };
    const OTHER: DeviceRef = DeviceRef {
        vid: 0x1234,
        pid: 0x5678,
    };

    /// A day chosen far from any daylight-saving change; its midnight, local.
    const MIDNIGHT: i64 = 1_780_000_000_000;

    fn at(hours: i64, minutes: i64, seconds: i64) -> Moment {
        let day_ms = ((hours * 60 + minutes) * 60 + seconds) * 1000;
        Moment {
            epoch_ms: MIDNIGHT + day_ms,
            day_ms,
        }
    }

    fn rule(id: &str, every: u32, seconds: u32) -> Rule {
        Rule {
            id: id.into(),
            name: String::new(),
            enabled: true,
            devices: vec![KEYBOARD],
            when: Trigger::Schedule {
                every,
                aligned: None,
                between: None,
            },
            show: Show {
                effect: format!("shipped:{id}"),
                params: serde_json::Map::new(),
            },
            lasts: Lasts { seconds },
        }
    }

    fn between(mut rule: Rule, from: &str, to: &str) -> Rule {
        let Trigger::Schedule { between, .. } = &mut rule.when;
        *between = Some(Window {
            from: TimeOfDay::parse(from).unwrap(),
            to: TimeOfDay::parse(to).unwrap(),
        });
        rule
    }

    struct Given {
        paused: bool,
        anchors: HashMap<String, i64>,
        dismissed: HashSet<(DeviceRef, String, i64)>,
        tried: HashMap<String, i64>,
    }

    impl Given {
        fn nothing() -> Self {
            Self {
                paused: false,
                anchors: HashMap::new(),
                dismissed: HashSet::new(),
                tried: HashMap::new(),
            }
        }

        fn resolve(&self, now: Moment, rules: &[Rule], device: DeviceRef) -> Option<Interruption> {
            resolve(
                &Context {
                    now,
                    rules,
                    paused: self.paused,
                    anchors: &self.anchors,
                    dismissed: &self.dismissed,
                    tried: &self.tried,
                },
                device,
            )
        }
    }

    /// The rule of the design, read from the file as written there.
    #[test]
    fn the_rule_of_the_design_reads_from_json() {
        let raw: serde_json::Value = serde_json::from_str(
            r#"{
              "id": "hourly", "name": "Hourly clock", "enabled": true,
              "devices": [{ "vid": 5426, "pid": 658 }],
              "when": { "kind": "schedule", "every": 3600, "aligned": true },
              "show": { "effect": "shipped:Clock", "params": {} },
              "for": { "seconds": 10 }
            }"#,
        )
        .unwrap();
        let parsed = parse(&[raw]);
        let rule = parsed[0].as_ref().expect("a valid rule");
        assert_eq!(rule.devices, vec![KEYBOARD]);
        assert_eq!(rule.lasts.seconds, 10);
        assert_eq!(rule.show.effect, "shipped:Clock");
    }

    /// A malformed rule comes back as an error beside the valid ones, rather
    /// than making the list unreadable.
    #[test]
    fn a_broken_rule_does_not_take_the_others_with_it() {
        let good = serde_json::to_value(rule("Clock", 3600, 10)).unwrap();
        let bad_time = serde_json::json!({
            "id": "night", "devices": [{ "vid": 5426, "pid": 658 }],
            "when": { "kind": "schedule", "every": 1, "between": { "from": "25:00", "to": "07:00" } },
            "show": { "effect": "hardware:off" }
        });
        let never = serde_json::to_value(rule("Never", 0, 10)).unwrap();

        let parsed = parse(&[good, bad_time, never]);
        assert!(parsed[0].is_ok());
        assert!(
            parsed[1].as_ref().unwrap_err().contains("25:00"),
            "{:?}",
            parsed[1]
        );
        assert!(
            parsed[2].as_ref().unwrap_err().contains("1 second"),
            "{:?}",
            parsed[2]
        );
    }

    /// Absent, `for` is ten seconds and `enabled` is off; a window writes back
    /// as it was read.
    #[test]
    fn defaults_and_times_of_day_round_trip() {
        let raw = serde_json::json!({
            "id": "night", "devices": [{ "vid": 5426, "pid": 658 }],
            "when": { "kind": "schedule", "every": 1, "between": { "from": "22:00", "to": "07:05" } },
            "show": { "effect": "hardware:off" }
        });
        let rule: Rule = serde_json::from_value(raw).unwrap();
        assert_eq!(rule.lasts.seconds, 10);
        assert!(!rule.enabled);
        let written = serde_json::to_value(&rule).unwrap();
        assert_eq!(written["when"]["between"]["to"], "07:05");
        assert!(
            written["when"].get("aligned").is_none(),
            "absent stays absent"
        );
    }

    #[test]
    fn aligned_by_default_when_the_period_divides_a_day() {
        assert!(default_aligned(3600));
        assert!(default_aligned(900));
        assert!(default_aligned(1));
        assert!(!default_aligned(7));
    }

    /// Every hour for ten seconds: on the hour, not a second before, not one
    /// after.
    #[test]
    fn an_hourly_rule_interrupts_on_the_hour_for_its_duration() {
        let rules = [rule("Clock", 3600, 10)];
        let given = Given::nothing();

        let found = given
            .resolve(at(10, 0, 5), &rules, KEYBOARD)
            .expect("on the hour");
        assert_eq!(found.since, at(10, 0, 0).epoch_ms);
        assert_eq!(found.until, Some(at(10, 0, 10).epoch_ms));
        assert_eq!(found.effect, "shipped:Clock");

        assert!(
            given.resolve(at(10, 0, 10), &rules, KEYBOARD).is_none(),
            "over"
        );
        assert!(
            given.resolve(at(9, 59, 59), &rules, KEYBOARD).is_none(),
            "not yet"
        );
        assert!(
            given.resolve(at(10, 0, 5), &rules, OTHER).is_none(),
            "another device"
        );
    }

    /// Every second for a second is one long occurrence, not a flicker: the
    /// same `since` from one second to the next, so nothing restarts.
    #[test]
    fn a_rule_lasting_its_period_goes_on_without_restarting() {
        let rules = [rule("Clock", 1, 1)];
        let given = Given::nothing();

        let first = given.resolve(at(14, 0, 0), &rules, KEYBOARD).unwrap();
        let later = given.resolve(at(14, 30, 7), &rules, KEYBOARD).unwrap();
        assert_eq!(first.since, later.since);
        assert_eq!(first.until, None, "no end in sight");
    }

    /// Not aligned, a schedule counts from when the rule was switched on, and a
    /// rule never seen switched on has not started.
    #[test]
    fn an_unaligned_rule_counts_from_its_anchor() {
        let rules = [rule("Clock", 7, 2)];
        let mut given = Given::nothing();
        assert!(given.resolve(at(12, 0, 0), &rules, KEYBOARD).is_none());

        given.anchors.insert("Clock".into(), at(12, 0, 1).epoch_ms);
        assert!(
            given.resolve(at(12, 0, 8), &rules, KEYBOARD).is_some(),
            "7 s after"
        );
        assert!(
            given.resolve(at(12, 0, 10), &rules, KEYBOARD).is_none(),
            "2 s later"
        );
        assert!(
            given.resolve(at(12, 0, 0), &rules, KEYBOARD).is_none(),
            "before it"
        );
    }

    /// Between 22:00 and 07:00, every second: the night, wrapping past midnight,
    /// until seven in the morning.
    #[test]
    fn a_night_window_wraps_past_midnight() {
        let rules = [between(rule("Off", 1, 1), "22:00", "07:00")];
        let given = Given::nothing();

        let evening = given.resolve(at(23, 0, 0), &rules, KEYBOARD).unwrap();
        assert_eq!(evening.since, at(22, 0, 0).epoch_ms);
        assert_eq!(evening.until, Some(at(7, 0, 0).epoch_ms + DAY_MS));

        let dawn = Moment {
            epoch_ms: at(6, 59, 59).epoch_ms + DAY_MS,
            day_ms: at(6, 59, 59).day_ms,
        };
        let morning = given.resolve(dawn, &rules, KEYBOARD).unwrap();
        assert_eq!(morning.since, evening.since, "the same night");

        assert!(
            given.resolve(at(7, 0, 0), &rules, KEYBOARD).is_none(),
            "seven"
        );
        assert!(
            given.resolve(at(12, 0, 0), &rules, KEYBOARD).is_none(),
            "noon"
        );
    }

    /// A window cuts an occurrence short: it ends when the window closes.
    #[test]
    fn a_window_closes_an_occurrence_early() {
        let rules = [between(rule("Clock", 3600, 600), "09:00", "10:05")];
        let given = Given::nothing();

        let found = given.resolve(at(10, 0, 30), &rules, KEYBOARD).unwrap();
        assert_eq!(found.until, Some(at(10, 5, 0).epoch_ms));
        assert!(given.resolve(at(10, 6, 0), &rules, KEYBOARD).is_none());
    }

    /// Two rules at once: the first in the list. Dismissed, it gives way to the
    /// second, and comes back at its next occurrence.
    #[test]
    fn the_first_rule_wins_and_a_dismissed_one_gives_way() {
        let rules = [rule("Clock", 3600, 10), rule("Rain", 60, 30)];
        let mut given = Given::nothing();

        assert_eq!(
            given.resolve(at(10, 0, 5), &rules, KEYBOARD).unwrap().rule,
            "Clock"
        );

        given
            .dismissed
            .insert((KEYBOARD, "Clock".into(), at(10, 0, 0).epoch_ms));
        assert_eq!(
            given.resolve(at(10, 0, 5), &rules, KEYBOARD).unwrap().rule,
            "Rain"
        );
        assert_eq!(
            given.resolve(at(11, 0, 5), &rules, KEYBOARD).unwrap().rule,
            "Clock"
        );
    }

    /// Switched off or paused, a rule does nothing; tried by hand, it runs once
    /// all the same.
    #[test]
    fn off_and_paused_rules_rest_but_a_try_runs() {
        let mut off = rule("Clock", 3600, 10);
        off.enabled = false;
        let rules = [off];
        let mut given = Given::nothing();
        assert!(given.resolve(at(10, 0, 5), &rules, KEYBOARD).is_none());

        given.paused = true;
        given.tried.insert("Clock".into(), at(15, 12, 0).epoch_ms);
        let tried = given
            .resolve(at(15, 12, 3), &rules, KEYBOARD)
            .expect("tried");
        assert_eq!(tried.until, Some(at(15, 12, 10).epoch_ms));
        assert!(
            given.resolve(at(15, 12, 10), &rules, KEYBOARD).is_none(),
            "once"
        );
    }

    /// Paused, even an enabled rule on the hour stays quiet.
    #[test]
    fn pausing_silences_every_schedule() {
        let rules = [rule("Clock", 3600, 10)];
        let mut given = Given::nothing();
        given.paused = true;
        assert!(given.resolve(at(10, 0, 5), &rules, KEYBOARD).is_none());
    }

    #[test]
    fn times_of_day_are_checked() {
        assert!(TimeOfDay::parse("07:00").is_ok());
        assert!(TimeOfDay::parse("23:59").is_ok());
        for bad in ["24:00", "7:00", "07:60", "0700", ""] {
            assert!(TimeOfDay::parse(bad).is_err(), "{bad}");
        }
    }
}
