/// Datum shift using grid interpolation.
use crate::authoring::*;

// ----- F O R W A R D --------------------------------------------------------------

fn fwd(op: &Op, _ctx: &dyn Context, operands: &mut dyn CoordinateSet) -> usize {
    let grids = &op.params.grids;
    let use_null_grid = op.params.boolean("null_grid");

    let mut successes = 0_usize;
    let n = operands.len();

    'points: for i in 0..n {
        let mut coord = operands.get_coord(i);

        for grid in grids.iter() {
            // Pick the first grid that contains the point.
            if grid.contains(coord) {
                // Geoid
                if grid.bands() == 1 {
                    let d = grid.interpolation(&coord, None);
                    coord[2] -= d[0];
                    operands.set_coord(i, &coord);
                    successes += 1;

                    continue 'points;
                }

                // Datum shift
                let d = grid.interpolation(&coord, None);
                coord[0] += d[0];
                coord[1] += d[1];
                operands.set_coord(i, &coord);
                successes += 1;

                continue 'points;
            }
        }

        if use_null_grid {
            continue;
        }

        // No grid found so we stomp on the coordinate
        operands.set_coord(i, &Coor4D::nan());
    }

    successes
}

// ----- I N V E R S E --------------------------------------------------------------

fn inv(op: &Op, _ctx: &dyn Context, operands: &mut dyn CoordinateSet) -> usize {
    let grids = &op.params.grids;
    let use_null_grid = op.params.boolean("null_grid");

    let mut successes = 0_usize;
    let n = operands.len();

    'points: for i in 0..n {
        let mut coord = operands.get_coord(i);

        for grid in grids.iter().rev() {
            // Pick the first grid that contains the point.
            if grid.contains(coord) {
                // Geoid
                if grid.bands() == 1 {
                    let t = grid.interpolation(&coord, None);
                    coord[2] += t[0];
                    operands.set_coord(i, &coord);
                    successes += 1;

                    continue 'points;
                }

                // Datum shift - here we need to iterate in the inverse case
                let mut t = coord - grid.interpolation(&coord, None);

                for _ in 0..10 {
                    let d = t - coord + grid.interpolation(&t, None);
                    t = t - d;
                    // i.e. d.dot(d).sqrt() < 1e-10
                    if d.dot(d) < 1e-20 {
                        break;
                    }
                }

                operands.set_coord(i, &t);
                successes += 1;

                continue 'points;
            }
        }

        if use_null_grid {
            continue;
        }

        // No grid found so we stomp on the coordinate
        operands.set_coord(i, &Coor4D::nan());
    }
    successes
}
// ----- C O N S T R U C T O R ------------------------------------------------------

// Example...
#[rustfmt::skip]
pub const GAMUT: [OpParameter; 3] = [
    OpParameter::Flag { key: "inv" },
    OpParameter::Text { key: "grids", default: None },
    OpParameter::Real { key: "padding", default: Some(0.5) },
];

pub fn new(parameters: &RawParameters, ctx: &dyn Context) -> Result<Op, Error> {
    let def = &parameters.definition;
    let mut params = ParsedParameters::new(parameters, &GAMUT)?;

    let grid_file_name = params.text("grids")?;
    for grid_name in grid_file_name.split(',') {
        if grid_name.ends_with("@null") {
            params.boolean.insert("null_grid");
            continue;
        }

        // TODO: Handle @optional grids

        let grid = ctx.get_grid(grid_name)?;
        params.grids.push(grid);
    }

    let fwd = InnerOp(fwd);
    let inv = InnerOp(inv);
    let descriptor = OpDescriptor::new(def, fwd, Some(inv));
    let steps = Vec::new();
    let id = OpHandle::new();

    Ok(Op {
        descriptor,
        params,
        steps,
        id,
    })
}

// ----- T E S T S ------------------------------------------------------------------

//#[cfg(with_plain)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gridshift() -> Result<(), Error> {
        let mut ctx = Plain::default();
        let op = ctx.op("gridshift grids=../../geodesy/datum/test.datum")?;
        let cph = Coor4D::geo(55., 12., 0., 0.);
        let mut data = [cph];

        ctx.apply(op, Fwd, &mut data)?;
        let res = data[0].to_geo();
        assert!((res[0] - 55.015278).abs() < 1e-6);
        assert!((res[1] - 12.003333).abs() < 1e-6);

        ctx.apply(op, Inv, &mut data)?;
        assert!((data[0][0] - cph[0]).abs() < 1e-10);
        assert!((data[0][1] - cph[1]).abs() < 1e-10);

        Ok(())
    }

    #[test]
    fn multiple_grids() -> Result<(), Error> {
        let mut ctx = Plain::default();
        let op = ctx
            .op("gridshift grids=../../geodesy/datum/test.datum,../../geodesy/datum/test.datum")?;
        let cph = Coor4D::geo(55., 12., 0., 0.);
        let mut data = [cph];

        ctx.apply(op, Fwd, &mut data)?;
        let res = data[0].to_geo();
        assert!((res[0] - 55.015278).abs() < 1e-6);
        assert!((res[1] - 12.003333).abs() < 1e-6);

        ctx.apply(op, Inv, &mut data)?;
        assert!((data[0][0] - cph[0]).abs() < 1e-10);
        assert!((data[0][1] - cph[1]).abs() < 1e-10);

        Ok(())
    }
}

// See additional tests in src/grid/mod.rs
