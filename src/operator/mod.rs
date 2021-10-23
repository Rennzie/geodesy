use crate::operator_construction::OperatorArgs;
use crate::operator_construction::OperatorConstructor;
use crate::Context;
use crate::CoordinateTuple;
use crate::GeodesyError;

// A HashMap would have been a better choice,for the OPERATOR_LIST, except
// for the annoying fact that it cannot be compile-time constructed
#[rustfmt::skip]
const OPERATOR_LIST: [(&str, OperatorConstructor); 13] = [
    ("adapt",      crate::operator::adapt::Adapt::operator),
    ("cart",       crate::operator::cart::Cart::operator),
    ("helmert",    crate::operator::helmert::Helmert::operator),
    ("lcc",        crate::operator::lcc::Lcc::operator),
    ("merc",       crate::operator::merc::Merc::operator),

    ("molodensky", crate::operator::molodensky::Molodensky::operator),
    ("dm",         crate::operator::nmea::Nmea::operator),
    ("nmea",       crate::operator::nmea::Nmea::operator),
    ("dms",        crate::operator::nmea::Nmea::dmsoperator),
    ("nmeass",     crate::operator::nmea::Nmea::dmsoperator),

    ("noop",       crate::operator::noop::Noop::operator),
    ("tmerc",      crate::operator::tmerc::Tmerc::operator),
    ("utm",        crate::operator::tmerc::Tmerc::utmoperator),
];

// Operator is a newtype around a Boxed trait OperatorCore,
// in order to be able to define methods on it.
// There's a good description of the crux here:
// https://stackoverflow.com/questions/35568871/is-it-possible-to-implement-methods-on-type-aliases
pub struct Operator(pub Box<dyn OperatorCore>);

impl Operator {
    /// The equivalent of the PROJ `proj_create()` function: Create an operator object
    /// from a text string.
    ///
    /// Example:
    /// ```rust
    /// // EPSG:1134 - 3 parameter Helmert, ED50/WGS84
    /// let mut ctx = geodesy::Context::new();
    /// let op = ctx.operation("helmert: {x: -87, y: -96, z: -120}");
    /// assert!(op.is_ok());
    /// let op = op.unwrap();
    /// let mut operands = [geodesy::CoordinateTuple::geo(55., 12.,0.,0.)];
    /// ctx.fwd(op, &mut operands);
    /// ctx.inv(op, &mut operands);
    /// assert!((operands[0][0].to_degrees() - 12.).abs() < 1.0e-10);
    /// ```
    pub fn new(definition: &str, ctx: &mut Context) -> Result<Operator, GeodesyError> {
        let definition = Context::gys_to_yaml(definition);

        let mut oa = OperatorArgs::new();
        oa.populate(&definition, "");
        operator_factory(&mut oa, ctx, 0)
    }
}

use core::fmt::Debug;
impl Debug for Operator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Operator {{{}}}", self.name())
    }
}

// Forwarding all OperatorCore methods to the boxed content
// Perhaps not necessary: We could deem Core low level and
// build a high level interface on top of Core.
impl OperatorCore for Operator {
    fn fwd(&self, ctx: &mut Context, operands: &mut [CoordinateTuple]) -> bool {
        self.0.fwd(ctx, operands)
    }

    fn inv(&self, ctx: &mut Context, operands: &mut [CoordinateTuple]) -> bool {
        self.0.inv(ctx, operands)
    }

    fn operate(
        &self,
        operand: &mut Context,
        operands: &mut [CoordinateTuple],
        forward: bool,
    ) -> bool {
        self.0.operate(operand, operands, forward)
    }

    fn invertible(&self) -> bool {
        self.0.invertible()
    }

    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn args(&self, step: usize) -> &OperatorArgs {
        self.0.args(step)
    }

    fn is_inverted(&self) -> bool {
        self.0.is_inverted()
    }
}

/// The core functionality exposed by the individual operator implementations.
/// This is not immediately intended for application program consumption: The
/// actual API is in the `impl`ementation for the [`Operator`](Operator) newtype struct,
/// which builds on this `trait` (which only holds `pub`ness in order to support
/// construction of user-defined operators).
pub trait OperatorCore {
    fn fwd(&self, ctx: &mut Context, operands: &mut [CoordinateTuple]) -> bool;

    // implementations must override at least one of {inv, invertible}
    #[allow(unused_variables)]
    fn inv(&self, ctx: &mut Context, operands: &mut [CoordinateTuple]) -> bool {
        ctx.error(self.name(), "Operator not invertible");
        false
    }

    fn invertible(&self) -> bool {
        true
    }

    fn is_noop(&self) -> bool {
        false
    }

    // operate fwd/inv, taking operator inversion into account.
    fn operate(&self, ctx: &mut Context, operands: &mut [CoordinateTuple], forward: bool) -> bool {
        // Short form of (inverted && !forward) || (forward && !inverted)
        if self.is_inverted() != forward {
            return self.fwd(ctx, operands);
        }
        // We do not need to check for self.invertible() here, since non-invertible
        // operators will return false as per the default-defined fn inv() above.
        self.inv(ctx, operands)
    }

    fn name(&self) -> &'static str {
        "UNKNOWN"
    }

    // number of steps. 0 unless the operator is a pipeline
    fn len(&self) -> usize {
        0_usize
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn args(&self, step: usize) -> &OperatorArgs;

    fn is_inverted(&self) -> bool;
}

mod adapt;
mod cart;
mod helmert;
mod lcc;
mod merc;
mod molodensky;
mod nmea;
mod noop;
mod pipeline;
mod tmerc;

pub(crate) fn operator_factory(
    args: &mut OperatorArgs,
    ctx: &mut Context,
    recursions: usize,
) -> Result<Operator, GeodesyError> {
    if recursions > 100 {
        ctx.error("Unknown", "Operator definition too deeply nested");
        return Err(GeodesyError::Recursion("(unknown)".to_string()));
    }

    // Look for runtime defined macros
    if let Some(definition) = ctx.locate_macro(&args.name) {
        let mut moreargs = args.spawn(definition);
        return operator_factory(&mut moreargs, ctx, recursions + 1);
    }

    // Is it a private asset (i.e. current directory) '.gys'-file?
    if let Some(mut definition) = Context::get_private_asset("transformations", &args.name, ".gys")
    {
        // First expand ARGS and translate to YAML...
        definition = expand_gys(&definition, args);
        // Then treat it just like any other macro!
        let mut moreargs = args.spawn(&definition);
        return operator_factory(&mut moreargs, ctx, recursions + 1);
    }

    // Is it a private asset (i.e. current directory) '.yml'-file?
    if let Some(definition) = Context::get_private_asset("transformations", &args.name, ".yml") {
        let mut moreargs = args.spawn(&definition);
        return operator_factory(&mut moreargs, ctx, recursions + 1);
    }

    // Is it a runtime defined operator?
    if let Some(op) = ctx.locate_operator(&args.name) {
        return op(args);
    }

    // Is it a shared asset '.gys'-file?
    if let Some(mut definition) = Context::get_shared_asset("transformations", &args.name, ".gys") {
        // First expand ARGS and translate to YAML...
        definition = expand_gys(&definition, args);
        // Then treat it just like any other macro!
        let mut moreargs = args.spawn(&definition);
        return operator_factory(&mut moreargs, ctx, recursions + 1);
    }

    // Is it a shared asset '.yml'-file?
    if let Some(definition) = Context::get_shared_asset("transformations", &args.name, ".yml") {
        let mut moreargs = args.spawn(&definition);
        return operator_factory(&mut moreargs, ctx, recursions + 1);
    }

    // If it is none of the above, it must be a built-in operator
    builtins(ctx, args)
}

/// Handle instantiation of built-in operators.
fn builtins(ctx: &mut Context, args: &mut OperatorArgs) -> Result<Operator, GeodesyError> {
    // Pipelines are not characterized by the name "pipeline", but simply by containing steps.
    if let Ok(steps) = args.numeric_value("_nsteps", 0.0) {
        if steps > 0.0 {
            match crate::operator::pipeline::Pipeline::new(args, ctx) {
                Err(err) => {
                    ctx.error(&err.to_string(), "pipeline");
                    return Err(err);
                }
                Ok(ok) => {
                    return Ok(Operator(Box::new(ok)));
                }
            }
        }
    }

    // The operator name may be prefixed with "builtin_", so operator-named
    // macros can delegate the hard work to the operators they shadow.
    let mut opname = args.name.clone().to_lowercase();
    if opname.starts_with("builtin_") {
        opname = opname.strip_prefix("builtin_").unwrap().to_string();
    }

    if let Some(index) = OPERATOR_LIST.iter().position(|&op| op.0 == opname) {
        return OPERATOR_LIST[index].1(args);
    }

    // Not a built in operator
    Err(GeodesyError::NotFound(opname))
}

/// Expand gys ARGS and translate to YAML
fn expand_gys(definition: &str, args: &mut OperatorArgs) -> String {
    let mut gysargs = String::new();
    for (key, value) in &args.args {
        if key == "ellps" || key == "_definition" {
            continue;
        }
        let elt = format!(" {key}:{value}", key = key, value = value);
        gysargs += &elt;
    }
    let definition = definition.replace("ARGS", &gysargs);

    // Then translate to YAML and return
    Context::gys_to_yaml(&definition)
}

// --------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::CoordinateTuple;
    use crate::GeodesyError;

    #[test]
    fn operator() {
        use crate::operator_construction::*;
        use crate::{Context, FWD, INV};
        let mut o = Context::new();

        // A non-existing operator
        let h = Operator::new("unimplemented_operator: {x: -87, y: -96, z: -120}", &mut o);
        assert!(h.is_err());

        // Define "hilmert" and "halmert" to circularly define each other, in order
        // to test the operator_factory recursion breaker
        assert!(o.register_macro("halmert", "hilmert: {}"));
        assert!(o.register_macro("hilmert", "halmert: {}"));
        if Operator::new("halmert: {x: -87, y: -96, z: -120}", &mut o).is_err() {
            assert!(o.report().contains("too deeply nested"));
        } else {
            panic!();
        }

        // Define "hulmert" as a macro forwarding its args to the "helmert" builtin
        assert!(o.register_macro("hulmert", "helmert: {x: ^x, y: ^y, z: ^z}"));

        // A plain operator: Helmert, EPSG:1134 - 3 parameter, ED50/WGS84
        let hh = Operator::new("helmert: {x: -87, y: -96, z: -120}", &mut o);
        assert!(hh.is_ok());
        let hh = hh.unwrap();

        // Same operator, defined through the "hulmert" macro
        let h = Operator::new("hulmert: {x: -87, y: -96, z: -120}", &mut o);
        assert!(h.is_ok());
        let h = h.unwrap();

        assert_eq!(hh.args(0).name, h.args(0).name);
        assert_eq!(hh.args(0).used, h.args(0).used);

        // Check that the "builtin_" prefix works properly: Shadow "helmert" with a
        // forwarding macro of the same name - without making trouble for later use
        assert!(o.register_macro("helmert", "builtin_helmert: {}"));

        let mut operands = [CoordinateTuple::raw(0., 0., 0., 0.)];

        h.operate(&mut o, operands.as_mut(), FWD);
        assert_eq!(operands[0].first(), -87.);
        assert_eq!(operands[0].second(), -96.);
        assert_eq!(operands[0].third(), -120.);

        h.operate(&mut o, operands.as_mut(), INV);
        assert_eq!(operands[0].first(), 0.);
        assert_eq!(operands[0].second(), 0.);
        assert_eq!(operands[0].third(), 0.);

        // A pipeline
        let pipeline = "ed50_etrs89: {
            steps: [
                cart: {ellps: intl},
                helmert: {x: -87, y: -96, z: -120},
                cart: {inv: true, ellps: GRS80}
            ]
        }";
        let h = Operator::new(pipeline, &mut o);
        assert!(h.is_ok());
        let h = h.unwrap();

        let mut operands = [CoordinateTuple::gis(12., 55., 100., 0.)];
        h.operate(&mut o, operands.as_mut(), FWD);
        let d = operands[0].to_degrees();
        let r = CoordinateTuple::raw(
            11.998815342385209,
            54.99938264895106,
            131.20240108577374,
            0.0,
        );

        assert!((d.first() - r.first()).abs() < 1.0e-10);
        assert!((d.second() - r.second()).abs() < 1.0e-10);
        assert!((d.third() - r.third()).abs() < 1.0e-8);

        // An externally defined version
        let h = Operator::new("ed50_etrs89", &mut o);
        assert!(h.is_ok());

        // Try to access it from data_local_dir (i.e. $HOME/share or somesuch)
        let h = Operator::new("ed50_etrs89", &mut o);
        // If we have access to "assets.zip" we expect to succeed
        if let Some(mut assets) = dirs::data_local_dir() {
            assets.push("geodesy");
            assets.push("assets.zip");
            if assets.exists() {
                assert!(h.is_ok());
                let mut operands = [CoordinateTuple::gis(12., 55., 100., 0.)];
                h.unwrap().operate(&mut o, operands.as_mut(), FWD);
                let d = operands[0].to_degrees();

                assert!((d.first() - r.first()).abs() < 1.0e-10);
                assert!((d.second() - r.second()).abs() < 1.0e-10);
                assert!((d.third() - r.third()).abs() < 1.0e-8);
            } else {
                assert!(h.is_err());
            }
        }

        // A parameterized macro pipeline version
        let pipeline_as_macro = "pipeline: {
            globals: {
                leftleft: ^left
            },
            steps: [
                cart: {ellps: ^leftleft},
                helmert: {x: ^x, y: ^y, z: ^z},
                cart: {inv: true, ellps: ^right}
            ]
        }";

        assert!(o.register_macro("geohelmert", pipeline_as_macro));
        let ed50_etrs89 = Operator::new(
            "geohelmert: {left: intl, right: GRS80, x: -87, y: -96, z: -120}",
            &mut o,
        );
        assert!(ed50_etrs89.is_ok());
        let ed50_etrs89 = ed50_etrs89.unwrap();
        let mut operands = [CoordinateTuple::gis(12., 55., 100., 0.)];

        ed50_etrs89.operate(&mut o, operands.as_mut(), FWD);
        let d = operands[0].to_degrees();

        assert!((d.first() - r.first()).abs() < 1.0e-10);
        assert!((d.second() - r.second()).abs() < 1.0e-10);
        assert!((d.third() - r.third()).abs() < 1.0e-8);

        ed50_etrs89.operate(&mut o, operands.as_mut(), INV);
        let d = operands[0].to_degrees();

        assert!((d.first() - 12.).abs() < 1.0e-10);
        assert!((d.second() - 55.).abs() < 1.0e-10);
        assert!((d.third() - 100.).abs() < 1.0e-8);
    }

    use super::Context;
    use super::Operator;
    use super::OperatorArgs;
    use super::OperatorCore;

    pub struct Nnoopp {
        args: OperatorArgs,
    }

    impl Nnoopp {
        fn new(args: &mut OperatorArgs) -> Result<Nnoopp, GeodesyError> {
            Ok(Nnoopp { args: args.clone() })
        }

        pub(crate) fn operator(args: &mut OperatorArgs) -> Result<Operator, GeodesyError> {
            let op = Nnoopp::new(args)?;
            Ok(Operator { 0: Box::new(op) })
        }
    }

    impl OperatorCore for Nnoopp {
        fn fwd(&self, _ctx: &mut Context, operands: &mut [CoordinateTuple]) -> bool {
            for coord in operands {
                coord[0] = 42.;
            }
            true
        }

        fn inv(&self, _ctx: &mut Context, operands: &mut [CoordinateTuple]) -> bool {
            for coord in operands {
                coord[0] = 24.;
            }
            true
        }

        fn name(&self) -> &'static str {
            "nnoopp"
        }

        fn is_inverted(&self) -> bool {
            false
        }

        fn args(&self, _step: usize) -> &OperatorArgs {
            &self.args
        }
    }

    #[test]
    fn user_defined_operator() {
        let mut ctx = Context::new();
        ctx.register_operator("nnoopp", Nnoopp::operator);

        let op = ctx.operation("nnoopp: {}").unwrap();
        let mut operands = [CoordinateTuple::raw(12., 55., 100., 0.)];
        let _aha = ctx.fwd(op, operands.as_mut());
        assert_eq!(operands[0][0], 42.);
    }
}