//! E2E runner (`testing/e2e/run.sh <target>`, `docs/11-quality-and-testing.md` §7).
//!
//! The scenario inventory is data (`scenarios.toml`). Each scenario declares its expected
//! outcome: PASS, or RED (error code `UNIMPLEMENTED`) with a named [`manifest::Reason`].
//! Matching is strict: any other outcome, including a RED for another reason, an
//! unexpected PASS, a panic or a timeout, is FAIL. Nothing is skipped: a need that is not
//! met (a component that does not exist, a surface no contract defines) is a RED with
//! that need as its reason. The components are separate processes located through
//! explicit paths in environment variables.

pub mod bodies;
pub mod harness;
pub mod manifest;
pub mod run;
