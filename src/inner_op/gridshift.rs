use super::*;
use crate::Provider;
use std::io::BufRead;


// ----- F O R W A R D --------------------------------------------------------------

fn fwd(_op: &Op, _prv: &dyn Provider, operands: &mut [Coord]) -> Result<usize, Error> {
    Ok(operands.len())
}

// ----- I N V E R S E --------------------------------------------------------------

fn inv(_op: &Op, _prv: &dyn Provider, operands: &mut [Coord]) -> Result<usize, Error> {
    Ok(operands.len())
}


// ----- C O N S T R U C T O R ------------------------------------------------------

// Example...
#[rustfmt::skip]
pub const GAMUT: [OpParameter; 3] = [
    OpParameter::Flag { key: "inv" },
    OpParameter::Text { key: "grids", default: None },
    OpParameter::Real { key: "padding", default: Some(0.5) },
];

pub fn new(parameters: &RawParameters, provider: &dyn Provider) -> Result<Op, Error> {
    let def = &parameters.definition;
    let mut params = ParsedParameters::new(parameters, &GAMUT)?;
    let grid_file_name = params.text("grids")?;

    let fwd = InnerOp(fwd);
    let inv = InnerOp(inv);
    let descriptor = OpDescriptor::new(def, fwd, Some(inv));
    let steps = Vec::<Op>::new();
    let id = OpHandle::default();

    Ok(Op {
        descriptor,
        params,
        steps,
        id,
    })
}


fn gravsoft_grid_reader(name: &str, provider: &dyn Provider) -> Result<Vec::<f64>, Error> {
    let buf = provider.get_blob(name)?;
    let all = std::io::BufReader::new(buf.as_slice());
    let mut grid = Vec::<f64>::new();

    for line in all.lines() {
        // Remove comments
        let line = line?;
        let line = line.split('#').collect::<Vec<_>>()[0];
        // Convert to f64
        for item in line.split_whitespace() {
            grid.push(item.parse::<f64>().unwrap_or(0.));
        }
    }
    Ok(grid)
}

// Clamp input to range min..max
fn clamp<T> (input: T, min: T, max: T) -> T
where T: PartialOrd<T>  {
    if input < min {
        return min;
    }
    if input > max {
        return max;
    }
    input
}

#[derive(Debug, Default)]
struct GridHeader {
    lat_0: f64,  /// Latitude of the first (typically northernmost) row of the grid
    lat_1: f64,  /// Latitude of the last (typically southernmost) row of the grid
    lon_0: f64,  /// Longitude of the first (typically westernmost) column of each row
    lon_1: f64,  /// Longitude of the last (typically easternmost) column of each row
    dlat: f64,   /// Signed distance between two consecutive rows
    dlon: f64,   /// Signed distance between two consecutive columns
    rows: usize,
    cols: usize,
    bands: usize,
    header_length: usize,
    last_valid_record_start: usize
}

impl GridHeader {
    fn gravsoft(grid: &[f64]) -> Result<Self, Error> {
        let lat_0 = grid[1];
        let lat_1 = grid[0];
        let lon_0 = grid[2];
        let lon_1 = grid[3];
        let dlat = -grid[4];
        let dlon = grid[5];
        let rows = ((lat_1 - lat_0)/dlat + 1.5).floor() as usize;
        let cols = ((lon_1 - lon_0)/dlon + 1.5).floor() as usize;
        let bands = (grid.len() - 6_usize) / (rows*cols);
        let header_length = 6;
        let last_valid_record_start = header_length + (rows * cols - 1) * bands;

        let elements = rows*cols*bands;
        if elements==0 || elements + header_length > grid.len() || bands < 1 {
            return Err(Error::General("Incomplete grid"))
        }

        Ok(GridHeader{
            lat_0,
            lat_1,
            lon_0,
            lon_1,
            dlat,
            dlon,
            rows,
            cols,
            bands,
            header_length,
            last_valid_record_start
        })
    }

    // Since we store the entire grid+header in a single vector, the interpolation
    // routine here looks strongly like a case of "writing Fortran 77 in Rust"
    pub fn interpolation(&self, coord: Coord, grid: &[f64]) -> Coord {
        // The interpolation coordinate relative to the grid origin
        let rlon = coord[0] - self.lon_0;
        let rlat = coord[1] - self.lat_0;

        // The (row, column) of the lower left node of the grid cell containing
        // coord or, in the case of extrapolation, the nearest cell inside the grid.
        let row = (rlat / self.dlat).floor() as i64;
        let col = (rlon / self.dlon).floor() as i64;

        assert_eq!((-1_f64 / -1_f64).floor(), 1_f64);

        let col = clamp(col, 0_i64, (self.cols - 2) as i64) as usize;
        let row = clamp(row, 1_i64, (self.rows - 1) as i64) as usize;

        // Index of the first band element of each corner value
        let ll = self.header_length + ((row + 0) * self.cols + col + 0) * self.bands;
        let lr = self.header_length + ((row + 0) * self.cols + col + 1) * self.bands;
        let ur = self.header_length + ((row - 1) * self.cols + col + 1) * self.bands;
        let ul = self.header_length + ((row - 1) * self.cols + col + 0) * self.bands;

        // Cell relative, cell unit coordinates in a right handed CS (hence .abs())
        let rlon = (coord[0] - (self.lon_0 + col as f64 * self.dlon)) / self.dlon.abs();
        let rlat = (coord[1] - (self.lat_0 + row as f64 * self.dlat)) / self.dlat.abs();
        dbg!((rlat, rlon));

        // Interpolate
        let mut left = Coord::origin();
        for i in 0..self.bands {
            left[i] = (1. - rlat) * grid[ll + i] + (0. + rlat) * grid[ul + i];
        }
        let mut right = Coord::origin();
        for i in 0..self.bands {
            right[i] = (1. - rlat) * grid[lr + i] + (0. + rlat) * grid[ur + i];
        }
        let mut result = Coord::origin();
        for i in 0..self.bands {
            result[i] = (1. - rlon) * left[i] + (0. + rlon) * right[i];
        }
        result
    }

}

// ----- T E S T S ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gravsoft() -> Result<(), Error> {
        let mut prv = Minimal::default();
        let op = prv.op("addone|addone|addone")?;
        let mut data = some_basic_coordinates();

        prv.apply(op, Fwd, &mut data)?;
        assert_eq!(data[0][0], 58.);
        assert_eq!(data[1][0], 62.);

        prv.apply(op, Inv, &mut data)?;
        assert_eq!(data[0][0], 55.);
        assert_eq!(data[1][0], 59.);

        let op = prv.op("addone|addone inv|addone")?;
        let mut data = some_basic_coordinates();
        assert_eq!(data[0][0], 55.);
        assert_eq!(data[1][0], 59.);

        prv.apply(op, Fwd, &mut data)?;
        assert_eq!(data[0][0], 56.);
        assert_eq!(data[1][0], 60.);

        prv.apply(op, Inv, &mut data)?;
        assert_eq!(data[0][0], 55.);
        assert_eq!(data[1][0], 59.);

        // Try to invoke garbage as a pipeline step
        assert!(matches!(
            prv.op("addone|addone|_garbage"),
            Err(Error::NotFound(_, _))
        ));

        Ok(())
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[allow(dead_code)]
    const HEADER: [f64; 6] = [54., 58., 8., 16., 1., 1.];

    #[rustfmt::skip]
    const GEOID: [f64; 5*9] = [
        58.08, 58.09, 58.10, 58.11, 58.12, 58.13, 58.14, 58.15, 58.16,
        57.08, 57.09, 57.10, 57.11, 57.12, 57.13, 57.14, 57.15, 57.16,
        56.08, 56.09, 56.10, 56.11, 56.12, 56.13, 56.14, 56.15, 56.16,
        55.08, 55.09, 55.10, 55.11, 55.12, 55.13, 55.14, 55.15, 55.16,
        54.08, 54.09, 54.10, 54.11, 54.12, 54.13, 54.14, 54.15, 54.16,
    ];

    #[allow(dead_code)]
    #[rustfmt::skip]
    const DATUM: [f64; 5*2*9] = [
        58., 08., 58., 09., 58., 10., 58., 11., 58., 12., 58., 13., 58., 14., 58., 15., 58., 16.,
        57., 08., 57., 09., 57., 10., 57., 11., 57., 12., 57., 13., 57., 14., 57., 15., 57., 16.,
        56., 08., 56., 09., 56., 10., 56., 11., 56., 12., 56., 13., 56., 14., 56., 15., 56., 16.,
        55., 08., 55., 09., 55., 10., 55., 11., 55., 12., 55., 13., 55., 14., 55., 15., 55., 16.,
        54., 08., 54., 09., 54., 10., 54., 11., 54., 12., 54., 13., 54., 14., 54., 15., 54., 16.,
    ];

    #[test]
    fn geoid_grid() -> Result<(), Error> {
        let mut grid = Vec::from(HEADER);
        grid.extend_from_slice(&GEOID[..]);
        let header = GridHeader::gravsoft(&grid)?;
        dbg!(&header);
        let c = Coord::raw(08., 58., 0., 0.);
        let n = header.interpolation(c, &grid);
        dbg!(n);
        assert_eq!(1, 0);
        Ok(())
    }

}
