//! Rules, and the resolver deciding what they ask of a device at an instant.
//!
//! **Pure**: no clock, no device, no disk. Everything that changes the answer —
//! the instant, the rules, the pause, what someone tried — comes in as an
//! argument, so every case below is a unit test rather than an evening in front of
//! a keyboard waiting for the hour (`docs/design/inputs-and-automations.md` §3.4).
//!
//! # When: a cron expression
//!
//! A rule says when it applies with a cron expression and for how long each time:
//! `0 * * * *` for 10 seconds is the hour; `0 22 * * *` for 9 hours, the night;
//! `0 9-18 * * 1-5`, office hours. One trigger that already says everything a
//! schedule of our own would have grown into, one option at a time.
//!
//! # When: nobody uses the computer
//!
//! `idle` applies once the session has had no input — no key, no mouse — for some
//! minutes, and lasts as long as that holds (#179). How long it has been comes in
//! with the context: the system's own count, never key capture.
//!
//! # When: a signal says so
//!
//! `signal` applies while a value another program sent equals the one the rule
//! names — held while it does, or flashed for the rule's duration at each send
//! (#108). What is held comes in with the context, like idleness.

use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use chrono::{DateTime, TimeZone};
use croner::Cron;
use serde::{Deserialize, Serialize};

use crate::signals::store::{valid_name, Held};
use crate::DeviceRef;

/// A rule, as `settings.json` holds it (§3.2).
///
/// > **Every hour** · **on** *DeathStalker V2 Pro* · **show** *Clock* · **for**
/// > *10 seconds*
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

/// When a rule applies.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Trigger {
    /// An occurrence starts each time the expression matches the local time: five
    /// fields, `minute hour day month weekday`, or six with seconds first.
    Cron { expr: String },
    /// Under way once nobody has used the computer for `minutes`, until someone
    /// does. The rule's duration does not apply, except to Try.
    Idle { minutes: u32 },
    /// Under way while the signal `name` equals `equals`, compared as text: from
    /// when that value was set, for as long as it holds — or, with `hold` off,
    /// for the rule's duration from each time it is received. There is no
    /// "becomes": the sender already chooses when to send.
    Signal {
        name: String,
        equals: String,
        #[serde(default = "holds")]
        hold: bool,
    },
}

fn holds() -> bool {
    true
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

impl Rule {
    /// The rule's expression, parsed; or why it cannot be, in English, for the log
    /// and the interface.
    pub fn cron(&self) -> Result<Cron, String> {
        match &self.when {
            Trigger::Cron { expr } => Cron::from_str(expr)
                .map_err(|e| format!("\"{expr}\" is not a cron expression: {e}")),
            Trigger::Idle { .. } => Err("an idle rule has no cron expression".into()),
            Trigger::Signal { .. } => Err("a signal rule has no cron expression".into()),
        }
    }

    /// What makes a rule unusable, in English.
    ///
    /// A broken rule stays in the file and does nothing (§3.4): dropping it would
    /// lose what someone wrote over a typo.
    pub fn problem(&self) -> Option<String> {
        match &self.when {
            Trigger::Cron { .. } => {
                if let Err(e) = self.cron() {
                    return Some(e);
                }
            }
            Trigger::Idle { minutes: 0 } => {
                return Some("idle means 1 minute or more without input".into())
            }
            Trigger::Idle { .. } => {}
            Trigger::Signal { name, .. } if !valid_name(name) => {
                return Some(format!("\"{name}\" is not a signal name"))
            }
            Trigger::Signal { .. } => {}
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

    fn lasts_ms(&self) -> i64 {
        i64::from(self.lasts.seconds) * 1000
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

/// An occurrence of a rule under way on a device.
#[derive(Clone, Debug, PartialEq)]
pub struct Interruption {
    pub rule: String,
    pub effect: String,
    pub params: serde_json::Map<String, serde_json::Value>,
    /// When this occurrence started, in epoch milliseconds.
    pub since: i64,
    /// When it ends, in epoch milliseconds.
    pub until: i64,
    /// It lasts while its trigger holds: `until` is only the next look, not an
    /// end anyone can announce.
    pub open: bool,
}

/// How far ahead an open occurrence — idle, a signal held — is known to last:
/// until the scheduler looks again, a second later.
const LOOK_MS: i64 = 1000;

/// Everything, besides the device, the answer depends on.
pub struct Context<'a, Tz: TimeZone> {
    /// Now, on the local clock — what a cron expression is read against.
    pub now: DateTime<Tz>,
    pub rules: &'a [Rule],
    /// Pause automations: no expression applies. A rule tried by hand still does.
    pub paused: bool,
    /// Rules tried by hand, and when, in epoch milliseconds: each runs once, for
    /// its duration, switched on or not, paused or not — the gesture wins.
    pub tried: &'a HashMap<String, i64>,
    /// How long since the last input in the session, in milliseconds; `None`
    /// where the system does not say, and an idle rule then never applies.
    pub idle: Option<i64>,
    /// The signals held now. Expired ones may still be there; they apply no
    /// more.
    pub signals: &'a BTreeMap<String, Held>,
}

/// Every occurrence under way on `device` now, first the one that should run.
///
/// A rule tried by hand comes first; then the enabled rules, in their order,
/// which is their priority (§3.2). Several can be under way at once: which one
/// actually runs also depends on what someone dismissed, and that is the
/// scheduler's to know — it takes the first it has not.
pub fn active<Tz: TimeZone>(context: &Context<Tz>, device: DeviceRef) -> Vec<Interruption> {
    let now = context.now.timestamp_millis();
    let usable = |rule: &&Rule| rule.devices.contains(&device) && rule.problem().is_none();

    let tried = context.rules.iter().filter(usable).filter_map(|rule| {
        let since = *context.tried.get(&rule.id)?;
        let until = since + rule.lasts_ms();
        (since <= now && now < until).then(|| interruption(rule, since, until, false))
    });

    let scheduled = context
        .rules
        .iter()
        .filter(|rule| !context.paused && rule.enabled)
        .filter(usable)
        .filter_map(|rule| match rule.when {
            Trigger::Cron { .. } => {
                let (since, until) = occurrence(rule, &context.now)?;
                Some(interruption(rule, since, until, false))
            }
            Trigger::Idle { minutes } => {
                let since = idle_since(minutes, context.idle?, now)?;
                Some(interruption(rule, since, now + LOOK_MS, true))
            }
            Trigger::Signal {
                ref name,
                ref equals,
                hold,
            } => {
                let held = context.signals.get(name)?;
                let alive = held.expires.is_none_or(|at| now < at);
                if !alive || held.value.as_text() != *equals {
                    return None;
                }
                if hold {
                    Some(interruption(rule, held.since, now + LOOK_MS, true))
                } else {
                    let until = held.received + rule.lasts_ms();
                    (now < until).then(|| interruption(rule, held.received, until, false))
                }
            }
        });

    tried.chain(scheduled).collect()
}

fn interruption(rule: &Rule, since: i64, until: i64, open: bool) -> Interruption {
    Interruption {
        rule: rule.id.clone(),
        effect: rule.show.effect.clone(),
        params: rule.show.params.clone(),
        since,
        until,
        open,
    }
}

/// The occurrence of a rule under way at `now`, as `(since, until)`: the last
/// time its expression matched, when that was less than its duration ago.
///
/// Read backwards from now on the local clock, so "at 02:30" on a night the clock
/// jumps from 02:00 to 03:00 is the library's to settle, not an arithmetic of
/// ours on milliseconds.
fn occurrence<Tz: TimeZone>(rule: &Rule, now: &DateTime<Tz>) -> Option<(i64, i64)> {
    let cron = rule.cron().ok()?;
    let started = cron.find_previous_occurrence(now, true).ok()?;
    let since = started.timestamp_millis();
    let until = since + rule.lasts_ms();
    (now.timestamp_millis() < until).then_some((since, until))
}

/// When an idle rule's occurrence started — the instant the session had been idle
/// for `minutes` — or `None` while it has not been that long.
///
/// The same instant at every look while nobody touches anything, which is what
/// makes the scheduler read one idle stretch as one run.
fn idle_since(minutes: u32, idle_ms: i64, now: i64) -> Option<i64> {
    let threshold = i64::from(minutes) * 60_000;
    (idle_ms >= threshold).then(|| now - idle_ms + threshold)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signals::store::Scalar;
    use chrono::FixedOffset;

    const KEYBOARD: DeviceRef = DeviceRef {
        vid: 0x1532,
        pid: 0x0292,
    };
    const OTHER: DeviceRef = DeviceRef {
        vid: 0x1234,
        pid: 0x5678,
    };

    /// Paris in summer: a fixed offset, so the tests read the same on any
    /// machine and any runner.
    fn at(day: u32, hours: u32, minutes: u32, seconds: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(2 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 9, day, hours, minutes, seconds)
            .unwrap()
    }

    fn ms(time: DateTime<FixedOffset>) -> i64 {
        time.timestamp_millis()
    }

    fn rule(id: &str, expr: &str, seconds: u32) -> Rule {
        Rule {
            id: id.into(),
            name: String::new(),
            enabled: true,
            devices: vec![KEYBOARD],
            when: Trigger::Cron { expr: expr.into() },
            show: Show {
                effect: format!("shipped:{id}"),
                params: serde_json::Map::new(),
            },
            lasts: Lasts { seconds },
        }
    }

    fn idle_rule(id: &str, minutes: u32) -> Rule {
        Rule {
            when: Trigger::Idle { minutes },
            ..rule(id, "0 * * * *", 10)
        }
    }

    struct Given {
        paused: bool,
        tried: HashMap<String, i64>,
        idle: Option<i64>,
        signals: BTreeMap<String, Held>,
    }

    impl Given {
        fn nothing() -> Self {
            Self {
                paused: false,
                tried: HashMap::new(),
                idle: Some(0),
                signals: BTreeMap::new(),
            }
        }

        fn first(
            &self,
            now: DateTime<FixedOffset>,
            rules: &[Rule],
            device: DeviceRef,
        ) -> Option<Interruption> {
            active(
                &Context {
                    now,
                    rules,
                    paused: self.paused,
                    tried: &self.tried,
                    idle: self.idle,
                    signals: &self.signals,
                },
                device,
            )
            .into_iter()
            .next()
        }
    }

    /// A rule as the tab writes it, read from the file.
    #[test]
    fn a_rule_reads_from_json() {
        let raw: serde_json::Value = serde_json::from_str(
            r#"{
              "id": "hourly", "name": "Hourly clock", "enabled": true,
              "devices": [{ "vid": 5426, "pid": 658 }],
              "when": { "kind": "cron", "expr": "0 * * * *" },
              "show": { "effect": "shipped:Clock", "params": {} },
              "for": { "seconds": 10 }
            }"#,
        )
        .unwrap();
        let parsed = parse(&[raw]);
        let rule = parsed[0].as_ref().expect("a valid rule");
        assert_eq!(rule.devices, vec![KEYBOARD]);
        assert_eq!(rule.lasts.seconds, 10);
    }

    /// A malformed rule comes back as an error beside the valid ones, rather
    /// than making the list unreadable; so does an expression that is not cron.
    #[test]
    fn a_broken_rule_does_not_take_the_others_with_it() {
        let good = serde_json::to_value(rule("Clock", "0 * * * *", 10)).unwrap();
        let bad_expr = serde_json::to_value(rule("Odd", "every hour", 10)).unwrap();
        let unreadable = serde_json::json!({ "id": "x", "when": "whenever", "show": 12 });
        let never = serde_json::to_value(rule("Never", "0 * * * *", 0)).unwrap();

        let parsed = parse(&[good, bad_expr, unreadable, never]);
        assert!(parsed[0].is_ok());
        assert!(
            parsed[1].as_ref().unwrap_err().contains("every hour"),
            "{:?}",
            parsed[1]
        );
        assert!(parsed[2].is_err());
        assert!(
            parsed[3].as_ref().unwrap_err().contains("1 second"),
            "{:?}",
            parsed[3]
        );
    }

    /// Absent, `for` is ten seconds and `enabled` is off.
    #[test]
    fn a_rule_says_little_and_gets_the_defaults() {
        let raw = serde_json::json!({
            "id": "hourly", "devices": [{ "vid": 5426, "pid": 658 }],
            "when": { "kind": "cron", "expr": "0 * * * *" },
            "show": { "effect": "shipped:Clock" }
        });
        let rule: Rule = serde_json::from_value(raw).unwrap();
        assert_eq!(rule.lasts.seconds, 10);
        assert!(!rule.enabled);
    }

    /// The hour for ten seconds: on the hour, not a second before, not one after.
    #[test]
    fn an_hourly_rule_interrupts_on_the_hour_for_its_duration() {
        let rules = [rule("Clock", "0 * * * *", 10)];
        let given = Given::nothing();

        let found = given
            .first(at(17, 10, 0, 5), &rules, KEYBOARD)
            .expect("on the hour");
        assert_eq!(found.since, ms(at(17, 10, 0, 0)));
        assert_eq!(found.until, ms(at(17, 10, 0, 10)));
        assert_eq!(found.effect, "shipped:Clock");

        assert!(
            given.first(at(17, 10, 0, 10), &rules, KEYBOARD).is_none(),
            "over"
        );
        assert!(
            given.first(at(17, 9, 59, 59), &rules, KEYBOARD).is_none(),
            "not yet"
        );
        assert!(
            given.first(at(17, 10, 0, 5), &rules, OTHER).is_none(),
            "another device"
        );
    }

    /// At 22:00 for nine hours is the night: still under way at 06:59 the next
    /// morning, over at seven.
    #[test]
    fn a_long_occurrence_runs_past_midnight() {
        let rules = [rule("Off", "0 22 * * *", 9 * 3600)];
        let given = Given::nothing();

        let night = given.first(at(17, 23, 30, 0), &rules, KEYBOARD).unwrap();
        assert_eq!(night.since, ms(at(17, 22, 0, 0)));
        assert_eq!(night.until, ms(at(18, 7, 0, 0)));

        let dawn = given.first(at(18, 6, 59, 59), &rules, KEYBOARD).unwrap();
        assert_eq!(dawn.since, night.since, "the same night");

        assert!(
            given.first(at(18, 7, 0, 0), &rules, KEYBOARD).is_none(),
            "seven"
        );
        assert!(
            given.first(at(18, 12, 0, 0), &rules, KEYBOARD).is_none(),
            "noon"
        );
    }

    /// Weekdays only: Thursday the 17th of September 2026 matches, Saturday the
    /// 19th does not.
    #[test]
    fn days_of_the_week_are_the_expression_s() {
        let rules = [rule("Clock", "0 9 * * 1-5", 60)];
        let given = Given::nothing();
        assert!(
            given.first(at(17, 9, 0, 30), &rules, KEYBOARD).is_some(),
            "Thursday"
        );
        assert!(
            given.first(at(19, 9, 0, 30), &rules, KEYBOARD).is_none(),
            "Saturday"
        );
    }

    /// Six fields: every thirty seconds, a five-second flash.
    #[test]
    fn seconds_are_a_sixth_field() {
        let rules = [rule("Flash", "*/30 * * * * *", 5)];
        let given = Given::nothing();
        assert!(given.first(at(17, 12, 0, 32), &rules, KEYBOARD).is_some());
        assert!(given.first(at(17, 12, 0, 36), &rules, KEYBOARD).is_none());
    }

    /// Every second for a second: an occurrence under way at every instant,
    /// each starting when the last ended — the scheduler strings them into one.
    #[test]
    fn back_to_back_occurrences_touch() {
        let rules = [rule("Clock", "* * * * * *", 1)];
        let given = Given::nothing();
        let first = given.first(at(17, 14, 0, 0), &rules, KEYBOARD).unwrap();
        let next = given.first(at(17, 14, 0, 1), &rules, KEYBOARD).unwrap();
        assert_eq!(next.since, first.until);
    }

    /// Two rules at once: both are under way, the first in the list first.
    #[test]
    fn rules_come_in_their_order() {
        let rules = [
            rule("Clock", "0 * * * *", 10),
            rule("Rain", "* * * * *", 30),
        ];
        let given = Given::nothing();
        let under_way = active(
            &Context {
                now: at(17, 10, 0, 5),
                rules: &rules,
                paused: false,
                tried: &given.tried,
                idle: given.idle,
                signals: &given.signals,
            },
            KEYBOARD,
        );
        let order: Vec<&str> = under_way.iter().map(|i| i.rule.as_str()).collect();
        assert_eq!(order, ["Clock", "Rain"]);
    }

    fn signal_rule(id: &str, name: &str, equals: &str, hold: bool) -> Rule {
        Rule {
            when: Trigger::Signal {
                name: name.into(),
                equals: equals.into(),
                hold,
            },
            ..rule(id, "0 * * * *", 5)
        }
    }

    fn given_signal(name: &str, value: Scalar, since: i64, received: i64) -> Given {
        let mut given = Given::nothing();
        given.signals.insert(
            name.into(),
            Held {
                value,
                since,
                received,
                expires: Some(received + 60_000),
            },
        );
        given
    }

    /// A rule as the tab writes it for a signal: held unless it says otherwise.
    #[test]
    fn a_signal_rule_reads_from_json_and_holds_by_default() {
        let raw = serde_json::json!({
            "id": "Build",
            "enabled": true,
            "devices": [{ "vid": 0x1532, "pid": 0x0292 }],
            "when": { "kind": "signal", "name": "build", "equals": "failed" },
            "show": { "effect": "shipped:Fixed gradient" }
        });
        let rule = parse(&[raw]).remove(0).unwrap();
        assert_eq!(rule.when, signal_rule("x", "build", "failed", true).when);
    }

    /// Held: from when the value was set, for as long as it is — a watcher
    /// re-sending it keeps the same start, so the scheduler reads one run.
    #[test]
    fn a_signal_rule_holds_while_the_value_does() {
        let set = at(17, 10, 0, 0).timestamp_millis();
        let now = at(17, 10, 0, 40);
        let given = given_signal("build", Scalar::Text("failed".into()), set, set + 30_000);
        let rules = [signal_rule("Build", "build", "failed", true)];

        let under_way = given.first(now, &rules, KEYBOARD).unwrap();
        assert_eq!(under_way.since, set);
        assert!(under_way.open);
        assert_eq!(under_way.until, now.timestamp_millis() + LOOK_MS);
    }

    #[test]
    fn another_value_or_no_value_applies_nothing() {
        let set = at(17, 10, 0, 0).timestamp_millis();
        let now = at(17, 10, 0, 1);
        let rules = [signal_rule("Build", "build", "failed", true)];

        let ok = given_signal("build", Scalar::Text("ok".into()), set, set);
        assert_eq!(ok.first(now, &rules, KEYBOARD), None);
        assert_eq!(Given::nothing().first(now, &rules, KEYBOARD), None);
    }

    #[test]
    fn an_expired_value_applies_no_more() {
        let set = at(17, 10, 0, 0).timestamp_millis();
        let given = given_signal("build", Scalar::Text("failed".into()), set, set);
        let rules = [signal_rule("Build", "build", "failed", true)];
        assert_eq!(given.first(at(17, 10, 1, 0), &rules, KEYBOARD), None);
    }

    /// Flashed: for the rule's duration from each receipt, so a doorbell rung
    /// again starts again.
    #[test]
    fn a_signal_flash_starts_at_each_receipt() {
        let set = at(17, 10, 0, 0).timestamp_millis();
        let rang = at(17, 10, 0, 20).timestamp_millis();
        let given = given_signal("doorbell", Scalar::Text("ring".into()), set, rang);
        let rules = [signal_rule("Door", "doorbell", "ring", false)];

        let flash = given.first(at(17, 10, 0, 23), &rules, KEYBOARD).unwrap();
        assert_eq!((flash.since, flash.until), (rang, rang + 5_000));
        assert!(!flash.open);
        assert_eq!(
            given.first(at(17, 10, 0, 26), &rules, KEYBOARD),
            None,
            "5 s later, over"
        );
    }

    #[test]
    fn a_number_sent_matches_the_text_a_rule_holds() {
        let set = at(17, 10, 0, 0).timestamp_millis();
        let given = given_signal("level", Scalar::Number(1.0), set, set);
        let rules = [signal_rule("Level", "level", "1", true)];
        assert!(given.first(at(17, 10, 0, 1), &rules, KEYBOARD).is_some());
    }

    #[test]
    fn a_signal_rule_names_a_valid_signal() {
        assert_eq!(
            signal_rule("Bad", "two words", "x", true)
                .problem()
                .as_deref(),
            Some("\"two words\" is not a signal name")
        );
    }

    /// Switched off or paused, a rule does nothing; tried by hand, it runs once
    /// all the same, ahead of everything scheduled.
    #[test]
    fn off_and_paused_rules_rest_but_a_try_runs() {
        let mut off = rule("Clock", "0 * * * *", 10);
        off.enabled = false;
        let rules = [off, rule("Rain", "* * * * *", 59)];
        let mut given = Given::nothing();
        assert_eq!(
            given
                .first(at(17, 10, 0, 5), &rules, KEYBOARD)
                .unwrap()
                .rule,
            "Rain"
        );

        given.paused = true;
        assert!(
            given.first(at(17, 10, 0, 5), &rules, KEYBOARD).is_none(),
            "paused"
        );

        given.tried.insert("Clock".into(), ms(at(17, 15, 12, 0)));
        let tried = given
            .first(at(17, 15, 12, 3), &rules, KEYBOARD)
            .expect("tried");
        assert_eq!(tried.rule, "Clock");
        assert_eq!(tried.until, ms(at(17, 15, 12, 10)));
        assert!(
            given.first(at(17, 15, 12, 10), &rules, KEYBOARD).is_none(),
            "once"
        );
    }

    /// An idle rule as the tab writes it, read from the file.
    #[test]
    fn an_idle_rule_reads_from_json() {
        let raw = serde_json::json!({
            "id": "away", "enabled": true, "devices": [{ "vid": 5426, "pid": 658 }],
            "when": { "kind": "idle", "minutes": 10 },
            "show": { "effect": "hardware:off" }
        });
        let parsed = parse(&[raw]);
        assert_eq!(
            parsed[0].as_ref().expect("a valid rule").when,
            Trigger::Idle { minutes: 10 }
        );
    }

    /// Ten minutes without input: not at nine fifty-nine, under way at ten, the
    /// same occurrence for as long as nothing is touched, over at the first input.
    #[test]
    fn an_idle_rule_holds_while_nobody_uses_the_computer() {
        let rules = [idle_rule("Away", 10)];
        let mut given = Given::nothing();

        given.idle = Some(9 * 60_000 + 59_000);
        assert!(
            given.first(at(17, 12, 0, 0), &rules, KEYBOARD).is_none(),
            "not yet"
        );

        given.idle = Some(10 * 60_000);
        let away = given
            .first(at(17, 12, 0, 0), &rules, KEYBOARD)
            .expect("ten minutes");
        assert_eq!(away.since, ms(at(17, 12, 0, 0)));
        assert!(away.open);

        given.idle = Some(25 * 60_000);
        let later = given.first(at(17, 12, 15, 0), &rules, KEYBOARD).unwrap();
        assert_eq!(later.since, away.since, "the same stretch");
        assert_eq!(
            later.until,
            ms(at(17, 12, 15, 1)),
            "known until the next look"
        );

        given.idle = Some(200);
        assert!(
            given.first(at(17, 12, 15, 1), &rules, KEYBOARD).is_none(),
            "someone is back"
        );
    }

    /// Where the system does not say how long it has been idle, an idle rule never
    /// applies; paused, it rests like any rule; zero minutes is not a rule.
    #[test]
    fn an_idle_rule_needs_the_system_to_say() {
        let rules = [idle_rule("Away", 10)];
        let mut given = Given::nothing();

        given.idle = None;
        assert!(
            given.first(at(17, 12, 0, 0), &rules, KEYBOARD).is_none(),
            "unavailable"
        );

        given.idle = Some(3_600_000);
        given.paused = true;
        assert!(
            given.first(at(17, 12, 0, 0), &rules, KEYBOARD).is_none(),
            "paused"
        );

        assert!(idle_rule("Never", 0)
            .problem()
            .is_some_and(|p| p.contains("1 minute")));
    }
}
