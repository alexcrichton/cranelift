//! RISC-V Settings.

use settings::{self, detail, Builder};
use std::fmt;

// Include code generated by `lib/cretonne/meta/gen_settings.py`. This file contains a public
// `Flags` struct with an impl for all of the settings defined in
// `lib/cretonne/meta/cretonne/settings.py`.
include!(concat!(env!("OUT_DIR"), "/settings-riscv.rs"));

#[cfg(test)]
mod tests {
    use super::{builder, Flags};
    use settings::{self, Configurable};

    #[test]
    fn display_default() {
        let shared = settings::Flags::new(&settings::builder());
        let b = builder();
        let f = Flags::new(&shared, &b);
        assert_eq!(f.to_string(),
                   "[riscv]\n\
                    supports_m = false\n\
                    supports_a = false\n\
                    supports_f = false\n\
                    supports_d = false\n\
                    enable_m = true\n\
                    enable_e = false\n");
        // Predicates are not part of the Display output.
        assert_eq!(f.full_float(), false);
    }

    #[test]
    fn predicates() {
        let shared = settings::Flags::new(&settings::builder());
        let mut b = builder();
        b.enable("supports_f").unwrap();
        b.enable("supports_d").unwrap();
        let f = Flags::new(&shared, &b);
        assert_eq!(f.full_float(), true);

        let mut sb = settings::builder();
        sb.set("enable_simd", "false").unwrap();
        let shared = settings::Flags::new(&sb);
        let mut b = builder();
        b.enable("supports_f").unwrap();
        b.enable("supports_d").unwrap();
        let f = Flags::new(&shared, &b);
        assert_eq!(f.full_float(), false);
    }
}
