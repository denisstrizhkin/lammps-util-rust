use clap::Parser;
use lammps_util_rust::dump_file::{DumpFile, DumpParsingError};
use lammps_util_rust::dump_snapshot::DumpSnapshot;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(value_name = "DUMP_INPUT")]
    dump_input_file: PathBuf,

    #[arg(value_name = "DUMP_FINAL")]
    dump_final_file: PathBuf,

    #[arg(short, long, value_name = "MAX_DEPTH (A)", default_value_t = 50.0)]
    max_depth: f64,

    #[arg(short, long, value_name = "THRESHOLD (A)", default_value_t = 3.0)]
    threshold: f64,

    #[arg(short, long, value_name = "STRIPE_WIDTH (A)", default_value_t = 5.43 / 2.0)]
    width: f64,
}

struct Stripes<'a> {
    ids: Vec<Vec<usize>>,
    snap: &'a DumpSnapshot,
}

fn check_threshold(a_x: f64, a_y: f64, b_x: f64, b_y: f64, threshold: f64) -> bool {
    let d_x = a_x - b_x;
    let d_y = a_y - b_y;
    d_x * d_x + d_y * d_y <= threshold * threshold
}

impl<'a> Stripes<'a> {
    pub fn new(snap: &'a DumpSnapshot, zero_lvl: f64, max_depth: f64, width: f64) -> Self {
        let snap_z = snap.get_property("z");
        let count = (max_depth / width).ceil() as usize;
        let mut ids = Vec::new();
        for _ in 0..count {
            ids.push(Vec::new());
        }
        for i in 0..snap_z.len() {
            let z = snap_z[i];
            if zero_lvl >= z && zero_lvl - max_depth < z {
                let j = ((zero_lvl - z) / width).floor() as usize;
                ids[j].push(i);
            }
        }
        Self { ids, snap }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    #[inline]
    pub fn get_xy(&self, i: usize) -> (f64, f64) {
        (
            self.snap.get_property("x")[i],
            self.snap.get_property("y")[i],
        )
    }

    pub fn get_missing_indexes(&self, snap: &Stripes, i: usize, threshold: f64) -> Vec<usize> {
        let mut indexes = Vec::new();
        for self_i in self.ids[i].iter().copied() {
            let (self_x, self_y) = self.get_xy(self_i);
            let mut missing = true;
            for snap_i in snap.ids[i].iter().copied() {
                let (snap_x, snap_y) = snap.get_xy(snap_i);
                if check_threshold(self_x, self_y, snap_x, snap_y, threshold) {
                    missing = false;
                    break;
                }
            }
            if missing {
                indexes.push(self_i);
            }
        }
        indexes
    }
}

fn get_crater_info(
    snap_input: &DumpSnapshot,
    snap_final: &DumpSnapshot,
    max_depth: f64,
    threshold: f64,
    width: f64,
) -> String {
    let zero_lvl = snap_input.get_zero_lvl();
    let stripes_input = Stripes::new(snap_input, zero_lvl, max_depth, width);
    let stripes_final = Stripes::new(snap_final, zero_lvl, max_depth, width);
    let mut crater_indexes = Vec::new();
    for i in 0..stripes_input.len() {
        let indexes = stripes_input.get_missing_indexes(&stripes_final, i, threshold);
        crater_indexes.extend(indexes);
    }
    let crater_count = crater_indexes.len();
    let mut surface_count = 0;
    let mut z_avg = 0.0;
    let mut z_min = f64::INFINITY;
    for i in crater_indexes {
        let z = snap_input.get_property("z")[i];
        if z > -2.4 * 0.707 + zero_lvl {
            surface_count += 1;
        }
        z_min = z_min.min(z - zero_lvl);
        z_avg += z - zero_lvl;
    }
    z_avg /= crater_count as f64;
    let volume = crater_count as f64 * 20.1;
    let surface = surface_count as f64 * 7.3712;
    format!("{crater_count} {volume} {surface} {z_avg} {z_min}")
}

fn main() -> Result<(), DumpParsingError> {
    let cli = Cli::parse();
    let dump_input = DumpFile::new(&cli.dump_input_file, &[])?;
    let dump_final = DumpFile::new(&cli.dump_final_file, &[])?;
    let info = get_crater_info(
        dump_input.get_snapshots()[0],
        dump_final.get_snapshots()[0],
        cli.max_depth,
        cli.threshold,
        cli.width,
    );
    println!("{info}");
    Ok(())
}