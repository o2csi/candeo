//! Automations: rules that interrupt the effect applied on a device for a while,
//! then give it back (#106, `docs/design/inputs-and-automations.md` §3).
//!
//! A device still runs exactly one effect. The one someone applied, remembered
//! in `activeEffects`, is its resting state; a rule replaces it for the length
//! of an occurrence and never rewrites that record, so when the rule ends the
//! device goes back to what someone chose.

pub mod resolver;
