//! Auto-update, following the Zed-style model used by the sibling Chitti Meet
//! desktop client.
//!
//! ## Trust model
//!
//! GitHub Releases stays the binary store. A signed `latest.json` per channel
//! is published separately and is the only thing the client interprets, so the
//! app never has to reason about GitHub's release semantics — notably that
//! GitHub's "latest release" endpoint returns only the newest NON-prerelease,
//! which is the wrong answer for anyone on beta.
//!
//! Two gates protect the user, in this order:
//!
//! 1. The manifest carries a detached Ed25519 signature verified against a key
//!    compiled into the binary. Whoever serves the metadata cannot substitute
//!    their own manifest.
//! 2. The artifact's SHA-256 is checked against the digest in that verified
//!    manifest before anything is unpacked.
//!
//! **This build fails closed.** With no key baked in, `check_for_update`
//! refuses rather than accepting an unsigned manifest — accepting one would
//! hand whoever hosts the metadata the ability to choose what code runs.
//!
//! ## What is here, and what is not
//!
//! Implemented and tested: the channel model, the manifest contract, signature
//! and checksum verification, and the decision rules.
//!
//! Not implemented: downloading, staging, and applying. Those replace the
//! installed application on disk and differ per platform — a helper process on
//! Windows because a running executable cannot replace itself, `hdiutil` plus
//! an rsync over the bundle on macOS, a directory swap on Linux. They also
//! cannot be meaningfully verified from here without the signing key and the
//! target platforms, so they are deliberately left for a pass that can be
//! tested end to end rather than written blind.

// The module is complete and tested but not yet consumed by the UI: there is
// no "Check for Updates" action until the apply half exists, and wiring one
// that can only ever report an update it cannot install would be worse than
// nothing.
#![allow(dead_code)]

pub mod channel;
pub mod check;
pub mod manifest;
pub mod verify;
