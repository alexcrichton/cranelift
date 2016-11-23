//! Intel Settings.

use settings::{self, detail, Builder};
use std::fmt;

// Include code generated by `lib/cretonne/meta/gen_settings.py`. This file contains a public
// `Flags` struct with an impl for all of the settings defined in
// `lib/cretonne/meta/cretonne/settings.py`.
include!(concat!(env!("OUT_DIR"), "/settings-intel.rs"));
