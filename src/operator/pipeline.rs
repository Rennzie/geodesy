use crate::CoordinateTuple;
use crate::GeodesyError;
use crate::GysArgs;
use crate::GysResource;
use crate::Operator;
use crate::OperatorCore;
use crate::Provider;
use crate::{FWD, INV};

#[derive(Debug)]
pub struct Pipeline {
    args: Vec<(String, String)>,
    pub steps: Vec<Operator>,
    inverted: bool,
}

impl Pipeline {
    pub fn new(
        args: &GysResource,
        rp: &dyn Provider,
        recursion_level: usize,
    ) -> Result<Operator, GeodesyError> {
        if recursion_level > 100 {
            return Err(GeodesyError::Recursion(format!("{:#?}", args)));
        }
        let mut margs = args.clone();
        let mut globals = GysArgs::new(&args.globals, "");

        // Is the pipeline itself inverted?
        let inverted = globals.flag("inv");

        // How many steps?
        let n = args.steps.len();

        // Redact the globals to eliminate the chaos-inducing "inv" and "name":
        // These are related to the pipeline itself, not its constituents.
        let globals: Vec<_> = args
            .globals
            .iter()
            .filter(|x| x.0 != "inv" && x.0 != "name")
            .cloned()
            .collect();
        let nextglobals = globals.clone();
        let mut steps = Vec::<Operator>::new();
        for step in &args.steps {
            // An embedded pipeline? (should not happen - elaborate!)
            if step.find('|').is_some() {
                continue;
            }

            let mut args = GysArgs::new(&nextglobals, step);

            let nextname = &args.value("name")?.unwrap_or_default();

            // A user defined operator?
            if let Some(op) = rp.get_user_defined_operator(nextname) {
                let args = GysResource::new(step, &nextglobals);
                let next = op(&args, rp)?;
                if n == 1 {
                    return Ok(next);
                }
                steps.push(next);
                continue;
            }

            // A macro? - args are now globals!
            if let Ok(mac) = rp.gys_definition("macros", nextname) {
                for arg in &args.locals {
                    let a = arg.clone();
                    args.globals.push(a);
                }
                let nextargs = GysResource::new(&mac, &args.globals);
                let next = Pipeline::new(&nextargs, rp, recursion_level + 1)?;
                if n == 1 {
                    return Ok(next);
                }
                steps.push(next);
                continue;
            }

            // If we did not find nextname among the resources it's probably a builtin
            let op = crate::operator::builtins::builtin(nextname)?;
            let args = GysResource::new(step, &nextglobals);
            let next = op(&args, rp)?;
            if n == 1 {
                return Ok(next);
            }
            steps.push(next);
            continue;
        }

        // makeshift clear text description
        margs.globals.clear();
        for step in margs.steps {
            margs.globals.push((String::from("step"), step));
        }

        let result = Pipeline {
            args: margs.globals,
            steps,
            inverted,
        };

        Ok(Operator(Box::new(result)))
    }
}

impl OperatorCore for Pipeline {
    fn fwd(&self, ctx: &dyn Provider, operands: &mut [CoordinateTuple]) -> bool {
        for step in &self.steps {
            if step.is_noop() {
                continue;
            }
            if !step.operate(ctx, operands, FWD) {
                return false;
            }
        }
        true
    }

    fn inv(&self, ctx: &dyn Provider, operands: &mut [CoordinateTuple]) -> bool {
        for step in self.steps.iter().rev() {
            if step.is_noop() {
                continue;
            }
            if !step.operate(ctx, operands, INV) {
                return false;
            }
        }
        true
    }

    fn len(&self) -> usize {
        self.steps.len()
    }

    fn args(&self, step: usize) -> &[(String, String)] {
        if step >= self.len() {
            return &self.args;
        }
        self.steps[step].args(0_usize)
    }

    fn name(&self) -> &'static str {
        "pipeline"
    }

    fn debug(&self) -> String {
        let mut repr = String::new();
        for step in &self.steps {
            repr += "\n";
            repr += &format!("{:#?}", step);
        }
        repr
    }

    fn is_inverted(&self) -> bool {
        self.inverted
    }
}

// --------------------------------------------------------------------------------

#[cfg(test)]
mod pipelinetests {
    use super::*;
    use crate::resource::SearchLevel;

    #[test]
    fn gys() -> Result<(), GeodesyError> {
        let rp = crate::Plain::new(SearchLevel::LocalPatches, true);
        let foo = rp
            .get_gys_definition_from_level(SearchLevel::LocalPatches, "macros", "foo")
            .unwrap();
        assert_eq!(foo.trim(), "bar");

        // This should be OK, since noop is a builtin
        let res = GysResource::from("noop pip");
        let p = Pipeline::new(&res, &rp, 0);
        assert!(p.is_ok());

        // This should be OK, due to "ignore" resolving to noop
        let res = GysResource::from("ignore pip");
        let p = Pipeline::new(&res, &rp, 0);
        assert!(p.is_ok());

        // This should fail, due to "baz" being undefined
        let res = GysResource::from("ignore pip|baz pop");
        let p = Pipeline::new(&res, &rp, 0);
        assert!(p.is_err());
        Ok(())
    }
}
