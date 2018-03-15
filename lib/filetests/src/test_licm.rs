//! Test command for testing the LICM pass.
//!
//! The `licm` test command runs each function through the LICM pass after ensuring
//! that all instructions are legal for the target.
//!
//! The resulting function is sent to `filecheck`.

use cretonne::ir::Function;
use cretonne;
use cretonne::print_errors::pretty_error;
use cton_reader::TestCommand;
use subtest::{SubTest, Context, Result, run_filecheck};
use std::borrow::Cow;
use std::fmt::Write;

struct TestLICM;

pub fn subtest(parsed: &TestCommand) -> Result<Box<SubTest>> {
    assert_eq!(parsed.command, "licm");
    if !parsed.options.is_empty() {
        Err(format!("No options allowed on {}", parsed))
    } else {
        Ok(Box::new(TestLICM))
    }
}

impl SubTest for TestLICM {
    fn name(&self) -> Cow<str> {
        Cow::from("licm")
    }

    fn is_mutating(&self) -> bool {
        true
    }

    fn run(&self, func: Cow<Function>, context: &Context) -> Result<()> {
        // Create a compilation context, and drop in the function.
        let mut comp_ctx = cretonne::Context::new();
        comp_ctx.func = func.into_owned();

        comp_ctx.flowgraph();
        comp_ctx.compute_loop_analysis();
        comp_ctx.licm(context.flags_or_isa()).map_err(|e| {
            pretty_error(&comp_ctx.func, context.isa, Into::into(e))
        })?;

        let mut text = String::new();
        write!(&mut text, "{}", &comp_ctx.func).map_err(
            |e| e.to_string(),
        )?;
        run_filecheck(&text, context)
    }
}
