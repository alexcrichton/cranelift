//! Run the tests in a single test file.

use std::borrow::Cow;
use std::path::Path;
use std::time;
use cretonne::ir::Function;
use cretonne::isa::TargetIsa;
use cretonne::settings::Flags;
use cretonne::verify_function;
use cton_reader::parse_test;
use cton_reader::IsaSpec;
use utils::read_to_string;
use filetest::{TestResult, new_subtest};
use filetest::subtest::{SubTest, Context, Result};

/// Load `path` and run the test in it.
///
/// If running this test causes a panic, it will propagate as normal.
pub fn run(path: &Path) -> TestResult {
    let started = time::Instant::now();
    let buffer = try!(read_to_string(path).map_err(|e| e.to_string()));
    let testfile = try!(parse_test(&buffer).map_err(|e| e.to_string()));
    if testfile.functions.is_empty() {
        return Err("no functions found".to_string());
    }

    // Parse the test commands.
    let mut tests = try!(testfile.commands.iter().map(new_subtest).collect::<Result<Vec<_>>>());

    // Flags to use for those tests that don't need an ISA.
    // This is the cumulative effect of all the `set` commands in the file.
    let flags = match testfile.isa_spec {
        IsaSpec::None(ref f) => f,
        IsaSpec::Some(ref v) => v.last().expect("Empty ISA list").flags(),
    };

    // Sort the tests so the mutators are at the end, and those that don't need the verifier are at
    // the front.
    tests.sort_by_key(|st| (st.is_mutating(), st.needs_verifier()));

    // Expand the tests into (test, flags, isa) tuples.
    let mut tuples = try!(test_tuples(&tests, &testfile.isa_spec, flags));

    // Isolate the last test in the hope that this is the only mutating test.
    // If so, we can completely avoid cloning functions.
    let last_tuple = match tuples.pop() {
        None => return Err("no test commands found".to_string()),
        Some(t) => t,
    };

    for (func, details) in testfile.functions {
        let mut context = Context {
            details: details,
            verified: false,
            flags: flags,
            isa: None,
        };

        for tuple in &tuples {
            try!(run_one_test(*tuple, Cow::Borrowed(&func), &mut context));
        }
        // Run the last test with an owned function which means it won't need to clone it before
        // mutating.
        try!(run_one_test(last_tuple, Cow::Owned(func), &mut context));
    }


    // TODO: Actually run the tests.
    Ok(started.elapsed())
}

// Given a slice of tests, generate a vector of (test, flags, isa) tuples.
fn test_tuples<'a>(tests: &'a [Box<SubTest>],
                   isa_spec: &'a IsaSpec,
                   no_isa_flags: &'a Flags)
                   -> Result<Vec<(&'a SubTest, &'a Flags, Option<&'a TargetIsa>)>> {
    let mut out = Vec::new();
    for test in tests {
        if test.needs_isa() {
            match *isa_spec {
                IsaSpec::None(_) => {
                    // TODO: Generate a list of default ISAs.
                    return Err(format!("test {} requires an ISA", test.name()));
                }
                IsaSpec::Some(ref isas) => {
                    for isa in isas {
                        out.push((&**test, isa.flags(), Some(&**isa)));
                    }
                }
            }
        } else {
            out.push((&**test, no_isa_flags, None));
        }
    }
    Ok(out)
}

fn run_one_test<'a>(tuple: (&'a SubTest, &'a Flags, Option<&'a TargetIsa>),
                    func: Cow<Function>,
                    context: &mut Context<'a>)
                    -> Result<()> {
    let (test, flags, isa) = tuple;
    let name = format!("{}({})", test.name(), func.name);

    context.flags = flags;
    context.isa = isa;

    // Should we run the verifier before this test?
    if !context.verified && test.needs_verifier() {
        try!(verify_function(&func).map_err(|e| e.to_string()));
        context.verified = true;
    }

    test.run(func, context).map_err(|e| format!("{}: {}", name, e))
}
