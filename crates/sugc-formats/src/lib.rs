//! Parsers and writers for the formats found on the user's own game disc.
//!
//! Format notes live in `knowledge/formats/`. Nothing in this crate embeds game data; tests
//! build their own synthetic inputs.

#![forbid(unsafe_code)]

pub mod fpg;
pub mod msf;
pub mod spr;
pub mod strtab;
pub mod texture;
