//! Shared settings module.
//!
//! This module defines data structures to access the settings defined in the meta language.
//!
//! Each settings group is translated to a `Flags` struct either in this module or in its
//! ISA-specific `settings` module. The struct provides individual getter methods for all of the
//! settings as well as computed predicate flags.
//!
//! The `Flags` struct is immutable once it has been created. A `Builder` instance is used to
//! create it.
//!
//! # Example
//! ```
//! use cretonne_codegen::settings::{self, Configurable};
//!
//! let mut b = settings::builder();
//! b.set("opt_level", "fastest");
//!
//! let f = settings::Flags::new(b);
//! assert_eq!(f.opt_level(), settings::OptLevel::Fastest);
//! ```

use constant_hash::{probe, simple_hash};
use isa::TargetIsa;
use std::fmt;
use std::result;
use std::boxed::Box;
use std::str;

/// A string-based configurator for settings groups.
///
/// The `Configurable` protocol allows settings to be modified by name before a finished `Flags`
/// struct is created.
pub trait Configurable {
    /// Set the string value of any setting by name.
    ///
    /// This can set any type of setting whether it is numeric, boolean, or enumerated.
    fn set(&mut self, name: &str, value: &str) -> Result<()>;

    /// Enable a boolean setting or apply a preset.
    ///
    /// If the identified setting isn't a boolean or a preset, a `BadType` error is returned.
    fn enable(&mut self, name: &str) -> Result<()>;
}

/// Collect settings values based on a template.
#[derive(Clone)]
pub struct Builder {
    template: &'static detail::Template,
    bytes: Box<[u8]>,
}

impl Builder {
    /// Create a new builder with defaults and names from the given template.
    pub fn new(tmpl: &'static detail::Template) -> Self {
        Self {
            template: tmpl,
            bytes: tmpl.defaults.into(),
        }
    }

    /// Extract contents of builder once everything is configured.
    pub fn state_for(&self, name: &str) -> &[u8] {
        assert_eq!(name, self.template.name);
        &self.bytes[..]
    }

    /// Set the value of a single bit.
    fn set_bit(&mut self, offset: usize, bit: u8, value: bool) {
        let byte = &mut self.bytes[offset];
        let mask = 1 << bit;
        if value {
            *byte |= mask;
        } else {
            *byte &= !mask;
        }
    }

    /// Apply a preset. The argument is a slice of (mask, value) bytes.
    fn apply_preset(&mut self, values: &[(u8, u8)]) {
        for (byte, &(mask, value)) in self.bytes.iter_mut().zip(values) {
            *byte = (*byte & !mask) | value;
        }
    }

    /// Look up a descriptor by name.
    fn lookup(&self, name: &str) -> Result<(usize, detail::Detail)> {
        match probe(self.template, name, simple_hash(name)) {
            Err(_) => Err(Error::BadName),
            Ok(entry) => {
                let d = &self.template.descriptors[self.template.hash_table[entry] as usize];
                Ok((d.offset as usize, d.detail))
            }
        }
    }
}

fn parse_bool_value(value: &str) -> Result<bool> {
    match value {
        "true" | "on" | "yes" | "1" => Ok(true),
        "false" | "off" | "no" | "0" => Ok(false),
        _ => Err(Error::BadValue),
    }
}

fn parse_enum_value(value: &str, choices: &[&str]) -> Result<u8> {
    match choices.iter().position(|&tag| tag == value) {
        Some(idx) => Ok(idx as u8),
        None => Err(Error::BadValue),
    }
}

impl Configurable for Builder {
    fn enable(&mut self, name: &str) -> Result<()> {
        use self::detail::Detail;
        let (offset, detail) = self.lookup(name)?;
        match detail {
            Detail::Bool { bit } => {
                self.set_bit(offset, bit, true);
                Ok(())
            }
            Detail::Preset => {
                self.apply_preset(&self.template.presets[offset..]);
                Ok(())
            }
            _ => Err(Error::BadType),
        }
    }

    fn set(&mut self, name: &str, value: &str) -> Result<()> {
        use self::detail::Detail;
        let (offset, detail) = self.lookup(name)?;
        match detail {
            Detail::Bool { bit } => {
                // Cannot currently propagate Result<()> up on functions returning ()
                // with the `?` operator
                self.set_bit(offset, bit, parse_bool_value(value)?);
            }
            Detail::Num => {
                self.bytes[offset] = value.parse().map_err(|_| Error::BadValue)?;
            }
            Detail::Enum { last, enumerators } => {
                self.bytes[offset] =
                    parse_enum_value(value, self.template.enums(last, enumerators))?;
            }
            Detail::Preset => return Err(Error::BadName),
        }
        Ok(())
    }
}

/// An error produced when changing a setting.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// No setting by this name exists.
    BadName,

    /// Type mismatch for setting (e.g., setting an enum setting as a bool).
    BadType,

    /// This is not a valid value for this setting.
    BadValue,
}

/// A result returned when changing a setting.
pub type Result<T> = result::Result<T, Error>;

/// A reference to just the boolean predicates of a settings object.
///
/// The settings objects themselves are generated and appear in the `isa/*/settings.rs` modules.
/// Each settings object provides a `predicate_view()` method that makes it possible to query
/// ISA predicates by number.
#[derive(Clone, Copy)]
pub struct PredicateView<'a>(&'a [u8]);

impl<'a> PredicateView<'a> {
    /// Create a new view of a precomputed predicate vector.
    ///
    /// See the `predicate_view()` method on the various `Flags` types defined for each ISA.
    pub fn new(bits: &'a [u8]) -> PredicateView {
        PredicateView(bits)
    }

    /// Check a numbered predicate.
    pub fn test(self, p: usize) -> bool {
        self.0[p / 8] & (1 << (p % 8)) != 0
    }
}

/// Implementation details for generated code.
///
/// This module holds definitions that need to be public so the can be instantiated by generated
/// code in other modules.
pub mod detail {
    use constant_hash;
    use std::fmt;

    /// An instruction group template.
    pub struct Template {
        /// Name of the instruction group.
        pub name: &'static str,
        /// List of setting descriptors.
        pub descriptors: &'static [Descriptor],
        /// Union of all enumerators.
        pub enumerators: &'static [&'static str],
        /// Hash table of settings.
        pub hash_table: &'static [u16],
        /// Default values.
        pub defaults: &'static [u8],
        /// Pairs of (mask, value) for presets.
        pub presets: &'static [(u8, u8)],
    }

    impl Template {
        /// Get enumerators corresponding to a `Details::Enum`.
        pub fn enums(&self, last: u8, enumerators: u16) -> &[&'static str] {
            let from = enumerators as usize;
            let len = usize::from(last) + 1;
            &self.enumerators[from..from + len]
        }

        /// Format a setting value as a TOML string. This is mostly for use by the generated
        /// `Display` implementation.
        pub fn format_toml_value(
            &self,
            detail: Detail,
            byte: u8,
            f: &mut fmt::Formatter,
        ) -> fmt::Result {
            match detail {
                Detail::Bool { bit } => write!(f, "{}", (byte & (1 << bit)) != 0),
                Detail::Num => write!(f, "{}", byte),
                Detail::Enum { last, enumerators } => {
                    if byte <= last {
                        let tags = self.enums(last, enumerators);
                        write!(f, "\"{}\"", tags[usize::from(byte)])
                    } else {
                        write!(f, "{}", byte)
                    }
                }
                // Presets aren't printed. They are reflected in the other settings.
                Detail::Preset { .. } => Ok(()),
            }
        }
    }

    /// The template contains a hash table for by-name lookup.
    impl<'a> constant_hash::Table<&'a str> for Template {
        fn len(&self) -> usize {
            self.hash_table.len()
        }

        fn key(&self, idx: usize) -> Option<&'a str> {
            let e = self.hash_table[idx] as usize;
            if e < self.descriptors.len() {
                Some(self.descriptors[e].name)
            } else {
                None
            }
        }
    }

    /// A setting descriptor holds the information needed to generically set and print a setting.
    ///
    /// Each settings group will be represented as a constant DESCRIPTORS array.
    pub struct Descriptor {
        /// Lower snake-case name of setting as defined in meta.
        pub name: &'static str,

        /// Offset of byte containing this setting.
        pub offset: u32,

        /// Additional details, depending on the kind of setting.
        pub detail: Detail,
    }

    /// The different kind of settings along with descriptor bits that depend on the kind.
    #[derive(Clone, Copy)]
    pub enum Detail {
        /// A boolean setting only uses one bit, numbered from LSB.
        Bool {
            /// 0-7.
            bit: u8,
        },

        /// A numerical setting uses the whole byte.
        Num,

        /// An Enum setting uses a range of enumerators.
        Enum {
            /// Numerical value of last enumerator, allowing for 1-256 enumerators.
            last: u8,

            /// First enumerator in the ENUMERATORS table.
            enumerators: u16,
        },

        /// A preset is not an individual setting, it is a collection of settings applied at once.
        ///
        /// The `Descriptor::offset` field refers to the `PRESETS` table.
        Preset,
    }

    impl Detail {
        /// Check if a detail is a Detail::Preset. Useful because the Descriptor
        /// offset field has a different meaning when the detail is a preset.
        pub fn is_preset(&self) -> bool {
            match *self {
                Detail::Preset => true,
                _ => false,
            }
        }
    }
}

// Include code generated by `meta/gen_settings.py`. This file contains a public `Flags` struct
// with an impl for all of the settings defined in `lib/codegen/meta/base/settings.py`.
include!(concat!(env!("OUT_DIR"), "/settings.rs"));

/// Wrapper containing flags and optionally a `TargetIsa` trait object.
///
/// A few passes need to access the flags but only optionally a target ISA. The `FlagsOrIsa`
/// wrapper can be used to pass either, and extract the flags so they are always accessible.
#[derive(Clone, Copy)]
pub struct FlagsOrIsa<'a> {
    /// Flags are always present.
    pub flags: &'a Flags,

    /// The ISA may not be present.
    pub isa: Option<&'a TargetIsa>,
}

impl<'a> From<&'a Flags> for FlagsOrIsa<'a> {
    fn from(flags: &'a Flags) -> FlagsOrIsa {
        FlagsOrIsa { flags, isa: None }
    }
}

impl<'a> From<&'a TargetIsa> for FlagsOrIsa<'a> {
    fn from(isa: &'a TargetIsa) -> FlagsOrIsa {
        FlagsOrIsa {
            flags: isa.flags(),
            isa: Some(isa),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Configurable;
    use super::Error::*;
    use super::{builder, Flags};
    use std::string::ToString;

    #[test]
    fn display_default() {
        let b = builder();
        let f = Flags::new(b);
        assert_eq!(
            f.to_string(),
            "[shared]\n\
             opt_level = \"default\"\n\
             enable_verifier = true\n\
             is_64bit = false\n\
             call_conv = \"fast\"\n\
             is_pic = false\n\
             colocated_libcalls = false\n\
             return_at_end = false\n\
             avoid_div_traps = false\n\
             is_compressed = false\n\
             enable_float = true\n\
             enable_nan_canonicalization = false\n\
             enable_simd = true\n\
             enable_atomics = true\n\
             baldrdash_prologue_words = 0\n\
             allones_funcaddrs = false\n\
             probestack_enabled = true\n\
             probestack_func_adjusts_sp = false\n\
             probestack_size_log2 = 12\n"
        );
        assert_eq!(f.opt_level(), super::OptLevel::Default);
        assert_eq!(f.enable_simd(), true);
        assert_eq!(f.baldrdash_prologue_words(), 0);
    }

    #[test]
    fn modify_bool() {
        let mut b = builder();
        assert_eq!(b.enable("not_there"), Err(BadName));
        assert_eq!(b.enable("enable_simd"), Ok(()));
        assert_eq!(b.set("enable_simd", "false"), Ok(()));

        let f = Flags::new(b);
        assert_eq!(f.enable_simd(), false);
    }

    #[test]
    fn modify_string() {
        let mut b = builder();
        assert_eq!(b.set("not_there", "true"), Err(BadName));
        assert_eq!(b.set("enable_simd", ""), Err(BadValue));
        assert_eq!(b.set("enable_simd", "best"), Err(BadValue));
        assert_eq!(b.set("opt_level", "true"), Err(BadValue));
        assert_eq!(b.set("opt_level", "best"), Ok(()));
        assert_eq!(b.set("enable_simd", "0"), Ok(()));

        let f = Flags::new(b);
        assert_eq!(f.enable_simd(), false);
        assert_eq!(f.opt_level(), super::OptLevel::Best);
    }
}
