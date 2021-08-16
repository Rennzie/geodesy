/*! Plonketi Plonk! !*/
//! How to append a postscript to the help message generated.
use std::path::PathBuf;
use structopt::StructOpt;

/// KP: The Rust Geodesy "Coordinate Processing" program is called kp rather than
/// the straightforward cp. Because cp is the Unix copy-command,
/// and because kp was the late Knud Poder (1925-2019), among colleagues and
/// collaborators rightfully considered the Nestor of computational
/// geodesy.
#[derive(StructOpt, Debug)]
#[structopt(name = "kp")]
struct Opt {
    /// Inverse
    #[structopt(short, long = "inv")]
    inverse: bool,

    /// Activate debug mode
    #[structopt(short, long)]
    debug: bool,

    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short, long, parse(from_occurrences))]
    verbose: u8,

    // Set speed
    //#[structopt(short, long, default_value = "42")]
    //speed: f64,
    /// Output file, stdout if not present
    #[structopt(short, long, parse(from_os_str))]
    output: Option<PathBuf>,

    // the long option will be translated by default to kebab case,
    // i.e. `--nb-cars`.
    // Number of cars
    // /#[structopt(short = "c", long)]
    //nb_cars: Option<i32>,
    /// Operation to apply
    #[structopt(name = "OPERATION", parse(from_os_str))]
    operation: PathBuf,

    /// Files to process
    #[structopt(name = "FILE", parse(from_os_str))]
    files: Vec<PathBuf>,
}
fn main() {
    let opt = Opt::from_args();
    println!("{:#?}", opt);

    // use std::env;
    use geodesy::CoordinateTuple as C;
    let mut ctx = geodesy::Context::new();

    if opt.debug {
        if let Some(dir) = dirs::data_local_dir() {
            eprintln!("data_local_dir: {}", dir.to_str().unwrap_or_default());
        }
    }

    // A pipeline in YAML
    let pipeline = "ed50_etrs89: {
        steps: [
            adapt: {from: neut_deg},
            cart: {ellps: intl},
            helmert: {x: -87, y: -96, z: -120},
            cart: {inv: true, ellps: GRS80},
            adapt: {to: neut_deg}
        ]
    }";

    // The same pipeline in Ghastly YAML Shorthand (GYS)
    let gys = "geo | cart ellps:intl | helmert x:-87 y:-96 z:-120 | cart inv ellps:GRS80 | geo inv";

    let op_yaml = ctx.operation(pipeline).unwrap();
    let op_gys = ctx.operation(gys).unwrap();

    let copenhagen = C::raw(55., 12., 0., 0.);
    let stockholm = C::raw(59., 18., 0., 0.);
    let mut yaml_data = [copenhagen, stockholm];
    let mut gys_data = [copenhagen, stockholm];

    ctx.fwd(op_yaml, &mut yaml_data);
    ctx.fwd(op_gys, &mut gys_data);

    println!("{:?}", yaml_data);
    println!("{:?}", gys_data);

    assert!(yaml_data[0].hypot3(&gys_data[0]) < 1e-16);
    assert!(yaml_data[1].hypot3(&gys_data[1]) < 1e-16);

    if false {
        if let Some(utm32) = ctx.operation("utm: {zone: 32}") {
            let copenhagen = C::geo(55., 12., 0., 0.);
            let stockholm = C::geo(59., 18., 0., 0.);
            let mut data = [copenhagen, stockholm];

            ctx.fwd(utm32, &mut data);
            println!("{:?}", data);
        }

        let coo = C([1., 2., 3., 4.]);
        println!("coo: {:?}", coo);

        let geo = C::geo(55., 12., 0., 0.);
        let gis = C::gis(12., 55., 0., 0.);
        assert_eq!(geo, gis);
        println!("geo: {:?}", geo.to_geo());

        // Some Nordic/Baltic capitals
        let nuk = C::gis(-52., 64., 0., 0.); // Nuuk
        let tor = C::gis(-7., 62., 0., 0.); // Tórshavn
        let cph = C::gis(12., 55., 0., 0.); // Copenhagen
        let osl = C::gis(10., 60., 0., 0.); // Oslo
        let sth = C::gis(18., 59., 0., 0.); // Stockholm
        let mar = C::gis(20., 60., 0., 0.); // Mariehamn
        let hel = C::gis(25., 60., 0., 0.); // Helsinki
        let tal = C::gis(25., 59., 0., 0.); // Tallinn
        let rga = C::gis(24., 57., 0., 0.); // Riga
        let vil = C::gis(25., 55., 0., 0.); // Vilnius

        // Gothenburg is not a capital, but it is strategically placed
        // approximately equidistant from OSL, CPH and STH, so it
        // deserves special treatment by getting its coordinate
        // from direct inline construction, which is perfectly
        // possible: A coordinate is just an array of four double
        // precision floats
        let got = C::geo(58., 12., 0., 0.0);

        let mut data_all = [nuk, tor, osl, cph, sth, mar, hel, tal, rga, vil];
        let mut data_utm32 = [osl, cph, got];

        // We loop over the full dataset, and add some arbitrary time information
        for (i, dimser) in data_all.iter_mut().enumerate() {
            dimser[3] = i as f64;
        }

        let utm32 = match ctx.operation("utm: {zone: 32}") {
            None => return println!("Awful error"),
            Some(op) => op,
        };

        ctx.fwd(utm32, &mut data_utm32);
        println!("utm32:");
        for coord in data_utm32 {
            println!("    {:?}", coord);
        }

        // Try to read predefined transformation from zip archive
        let pladder = match ctx.operation("ed50_etrs89") {
            None => return println!("Awful error"),
            Some(op) => op,
        };
        ctx.fwd(pladder, &mut data_all);
        println!("etrs89:");
        for coord in data_all {
            println!("    {:?}", coord.to_geo());
        }

        let pipeline = "ed50_etrs89: {
        steps: [
            cart: {ellps: intl},
            helmert: {x: -87, y: -96, z: -120},
            cart: {inv: true, ellps: GRS80}
        ]
    }";

        let ed50_etrs89 = match ctx.operation(pipeline) {
            None => return println!("Awful error"),
            Some(op) => op,
        };

        ctx.inv(ed50_etrs89, &mut data_all);
        println!("etrs89:");
        for coord in data_all {
            println!("    {:?}", coord.to_geo());
        }
    }
}