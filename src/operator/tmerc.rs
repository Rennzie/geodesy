//! Transverse Mercator

// Renovering af Poder/Engsager tmerc i B:\2019\Projects\FIRE\tramp\tramp\tramp.c
// Detaljer i C:\Users\B004330\Downloads\2.1.2 A HIGHLY ACCURATE WORLD WIDE ALGORITHM FOR THE TRANSVE (1).doc

use super::Operand;
use super::OperatorArgs;
use super::OperatorCore;
use crate::Ellipsoid;

#[derive(Debug)]
pub struct Tmerc {
    ellps: Ellipsoid,
    inverted: bool,
    eps: f64,
    k_0: f64,
    lon_0: f64,
    lat_0: f64,
    x_0: f64,
    y_0: f64,
    args: OperatorArgs,
}

impl Tmerc {
    pub fn new(args: &mut OperatorArgs) -> Result<Tmerc, String> {
        let ellps = Ellipsoid::named(&args.value("ellps", "GRS80"));
        let inverted = args.flag("inv");
        let k_0 = args.numeric_value("Tmerc", "k_0", 1.)?;
        let lon_0 = args.numeric_value("Tmerc", "lon_0", 0.)?.to_radians();
        let lat_0 = args.numeric_value("Tmerc", "lat_0", 0.)?.to_radians();
        let x_0 = args.numeric_value("Tmerc", "x_0", 0.)?;
        let y_0 = args.numeric_value("Tmerc", "y_0", 0.)?;
        let eps = ellps.second_eccentricity_squared();
        let args = args.clone();
        Ok(Tmerc {
            ellps,
            inverted,
            args,
            k_0,
            lon_0,
            lat_0,
            x_0,
            y_0,
            eps,
        })
    }

    pub fn utm(args: &mut OperatorArgs) -> Result<Tmerc, String> {
        let ellps = Ellipsoid::named(&args.value("ellps", "GRS80"));
        let zone = args.numeric_value("Utm", "zone", f64::NAN)?;
        let inverted = args.flag("inv");
        let k_0 = 0.9996;
        let lon_0 = (-183. + 6. * zone).to_radians();
        let lat_0 = 0.;
        let x_0 = 500_000.;
        let y_0 = 0.;
        let eps = ellps.second_eccentricity_squared();
        let args = args.clone();

        Ok(Tmerc {
            ellps,
            inverted,
            args,
            k_0,
            lon_0,
            lat_0,
            x_0,
            y_0,
            eps,
        })
    }
}

#[allow(non_snake_case)]
impl OperatorCore for Tmerc {
    // Forward transverse mercator, following Bowring
    fn fwd(&self, operand: &mut Operand) -> bool {
        let lat = operand.coord.1;
        let c = lat.cos();
        let s = lat.sin();
        let cc = c * c;
        let ss = s * s;

        let dlon = operand.coord.0 - self.lon_0;
        let oo = dlon * dlon;

        let N = self.ellps.prime_vertical_radius_of_curvature(lat);
        let z = self.eps * dlon.powi(3) * c.powi(5) / 6.;
        let sd2 = (dlon / 2.).sin();

        let theta_2 = (2. * s * c * sd2 * sd2).atan2(ss + cc * dlon.cos());

        // Easting
        let sd = dlon.sin();
        operand.coord.0 =
            self.x_0 + self.k_0 * N * ((c * sd).atanh() + z * (1. + oo * (36. * cc - 29.) / 10.));

        // Northing
        let m = self.ellps.meridional_distance(lat, true);
        let znos4 = z * N * dlon * s / 4.;
        let ecc = 4. * self.eps * cc;
        operand.coord.1 =
            self.y_0 + self.k_0 * (m + N * theta_2 + znos4 * (9. + ecc + oo * (20. * cc - 11.)));

        true
    }

    // Forward transverse mercator, following Bowring (1989)
    fn inv(&self, operand: &mut Operand) -> bool {
        // Footpoint latitude, i.e. the latitude of a point on the central meridian
        // having the same northing as the point of interest
        let lat = self
            .ellps
            .meridional_distance((operand.coord.1 - self.y_0) / self.k_0, false);
        let t = lat.tan();
        let c = lat.cos();
        let cc = c * c;
        let N = self.ellps.prime_vertical_radius_of_curvature(lat);
        let x = (operand.coord.0 - self.x_0) / (self.k_0 * N);
        let xx = x * x;
        let theta_4 = x.sinh().atan2(c);
        let theta_5 = (t * theta_4.cos()).atan();

        // Latitude
        let xet = xx * xx * self.eps * t / 24.;
        operand.coord.1 = self.lat_0 + (1. + cc * self.eps) * (theta_5 - xet * (9. - 10. * cc))
            - self.eps * cc * lat;

        // Longitude
        let approx = self.lon_0 + theta_4;
        let coef = self.eps / 60. * xx * x * c;
        operand.coord.0 = approx - coef * (10. - 4. * xx / cc + xx * cc);
        true
    }

    fn name(&self) -> &'static str {
        "tmerc"
    }

    fn is_inverted(&self) -> bool {
        self.inverted
    }

    fn args(&self, _step: usize) -> &OperatorArgs {
        &self.args
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn utm() {
        use crate::{CoordinateTuple, Ellipsoid, Operand, Operator, OperatorCore};

        // Test the UTM implementation
        let op = Operator::new("utm: {zone: 32}").unwrap();

        let mut operand = Operand::new();
        let geo = CoordinateTuple::deg(12., 55., 100., 0.);
        operand.coord = geo;

        // Validation value from PROJ:
        // echo 12 55 0 0 | cct -d18 +proj=utm +zone=32
        let utm_proj = CoordinateTuple(691_875.632_139_661, 6_098_907.825_005_012, 100., 0.);
        assert!(op.fwd(&mut operand));
        assert!(operand.coord.hypot2(&utm_proj) < 1e-5);

        // Roundtrip...
        assert!(op.inv(&mut operand));

        // The latitude roundtrips beautifully, at better than 0.1 mm
        assert!((operand.coord.1.to_degrees() - 55.0).abs() * 111_000_000. < 0.05);
        // And the longitude even trumps that by a factor of 10.
        assert!((operand.coord.0.to_degrees() - 12.0).abs() * 56_000_000. < 0.005);

        // So also the geodesic distance is smaller than 0.1 mm
        let ellps = Ellipsoid::default();
        assert!(ellps.distance(&operand.coord, &geo) < 1e-4);

        // Test a Greenland extreme value (a zone 19 point projected in zone 24)
        let op = Operator::new("utm: {zone: 24}").unwrap();
        let geo = CoordinateTuple::deg(-72., 80., 100., 0.);
        operand.coord = geo;
        // Roundtrip...
        op.fwd(&mut operand);
        op.inv(&mut operand);
        assert!(ellps.distance(&operand.coord, &geo) < 1.05);

        operand.coord.1 = operand.coord.1.to_degrees();
        operand.coord.0 = operand.coord.0.to_degrees();
        assert!((operand.coord.1 - 80.0).abs() * 111_000. < 1.02);
        assert!((operand.coord.0 + 72.0).abs() * 20_000. < 0.04);

        // i.e. Bowring's verion is much better than Snyder's:
        // echo -72 80 0 0 | cct +proj=utm +approx +zone=24 +ellps=GRS80 | cct -I +proj=utm +approx +zone=24 +ellps=GRS80
        // -71.9066920547   80.0022281660        0.0000        0.0000
        //
        // But obviously much worse than Poder/Engsager's:
        // echo -72 80 0 0 | cct +proj=utm +zone=24 +ellps=GRS80 | cct -I +proj=utm +zone=24 +ellps=GRS80
        // -72.0000000022   80.0000000001        0.0000        0.0000
    }

    #[test]
    fn tmerc() {
        use super::*;

        // Test the plain tmerc, by reimplementing the UTM above manually
        let tmerc = "tmerc: {k_0: 0.9996, lon_0: 9, x_0: 500000}";
        let mut args = OperatorArgs::global_defaults();
        args.populate(&tmerc, "");
        let op = Tmerc::new(&mut args).unwrap();

        let mut operand = Operand::new();
        operand.coord = crate::CoordinateTuple(12f64.to_radians(), 55f64.to_radians(), 100., 0.);
        op.fwd(&mut operand);

        // Validation value from PROJ:
        // echo 12 55 0 0 | cct -d18 +proj=utm +zone=32
        assert!((operand.coord.0 - 691875.6321396606508642440).abs() < 1e-5);
        assert!((operand.coord.1 - 6098907.825005011633038521).abs() < 1e-5);
    }
}