//! Headless entry point: run an algorithm over a whole TIFF, convert an image, or check
//! the atlas assets. Useful for batch work and for validating against MATLAB output.

use anyhow::{bail, Context, Result};
use microcount::core::analysis::algorithms::{self, RegionInput};
use microcount::core::analysis::data::{summarise, write_tables};
use microcount::core::analysis::render::Renderer;
use microcount::core::analysis::{Algorithm, RegionSettings, Settings};
use microcount::core::atlas::Atlas;
use microcount::core::image::{Mask, Rect};
use microcount::core::tiff::TiffFile;
use microcount::core::Cancel;
use std::path::PathBuf;
use std::time::Instant;

const USAGE: &str = "usage:
  microcount-cli run <image.tiff> [--algo micro-fluor|micro-dab|micro-plaque|astro-fluor|astro-dab|astro-single]
                     [--cell N] [--co N] [--px H,W] [--rect X,Y,W,H] [--settings branch,comarker,maxsize,overlap,soma]
                     [--out DIR]
  microcount-cli atlas <assets-dir>";

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

fn floats(s: &str) -> Result<Vec<f64>> {
    s.split(',').map(|v| v.trim().parse::<f64>().context("expected comma separated numbers")).collect()
}

fn run(args: &[String]) -> Result<()> {
    let path = PathBuf::from(args.first().context(USAGE)?);
    let algo_name = arg(args, "--algo").unwrap_or("micro-fluor");
    let algo = Algorithm::ALL.into_iter().find(|a| a.cli_name() == algo_name).context("unknown --algo")?;
    let cell: usize = arg(args, "--cell").unwrap_or("1").parse()?;
    let co: usize = arg(args, "--co").unwrap_or("2").parse()?;
    let px = match arg(args, "--px") {
        Some(s) => {
            let v = floats(s)?;
            [v[0], *v.get(1).unwrap_or(&v[0])]
        }
        None => [1.0, 1.0],
    };
    let mut rs = RegionSettings::default();
    if let Some(s) = arg(args, "--settings") {
        let v = floats(s)?;
        if v.len() != 5 {
            bail!("--settings needs 5 values");
        }
        rs = RegionSettings {
            cell_threshold: v[0],
            comarker_threshold: v[1],
            max_comarker_size: v[2] as u64,
            overlap_percentage: v[3],
            soma_threshold: v[4],
        };
    }
    let out = arg(args, "--out").map(PathBuf::from);

    let t0 = Instant::now();
    let tf = TiffFile::open(&path)?;
    let n_ch = tf.channel_count();
    let (w, h) = (tf.pages[0].width, tf.pages[0].height);
    let rect = match arg(args, "--rect") {
        Some(s) => {
            let v = floats(s)?;
            Rect::new(v[0] as usize, v[1] as usize, v[2] as usize, v[3] as usize)
        }
        None => Rect::new(0, 0, w, h),
    };
    let (w, h) = (rect.w, rect.h);
    let pick = |c: usize| tf.read_channel((c.min(n_ch)).saturating_sub(1), rect);
    let needs = algo.needs();
    let input = RegionInput {
        mask: Mask::filled(w, h, true),
        n_pixels: (w * h) as u64,
        cell: if needs.cell { Some(pick(cell)?) } else { None },
        co: if needs.co { Some(pick(co)?) } else { None },
        rgb: if needs.rgb { Some(tf.read_region::<u16>(0, rect)?) } else { None },
        full_range: if tf.pages[0].bps == 8 { 255 } else { 65535 },
    };
    eprintln!("read {w}x{h} ({n_ch} channels) in {:.2}s", t0.elapsed().as_secs_f64());
    let settings = Settings::new(&rs, px);
    let t1 = Instant::now();
    let mut data = algorithms::run(algo, &input, &settings, None, &Cancel::default())?;
    if algo == Algorithm::MicroPlaque {
        data = algorithms::split_plaque(&data, &settings).0;
    }
    eprintln!("{} in {:.2}s", algo.label(), t1.elapsed().as_secs_f64());
    let (result, tables) = summarise(&data, px);
    let c = &data.cells;
    eprintln!(
        "debug: soma_px {} binary_px {} co_px {} seeds {} n_cells {} skel_px {} detected {} sum_cell_area {} n_branches {}",
        data.soma_mask.count(),
        data.iba1_area_px,
        data.cd68_mask.count(),
        c.n_seeds,
        c.n_cells,
        c.skeleton.data.iter().filter(|&&v| v > 0).count(),
        c.detected.count(),
        c.cell_areas.iter().sum::<f64>(),
        c.branch_lengths.iter().map(|b| b.len()).sum::<usize>()
    );
    println!("{}", serde_json::to_string_pretty(&result)?);
    if let Some(dir) = out {
        std::fs::create_dir_all(&dir)?;
        let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        write_tables(&tables, &dir.join(format!("{stem}.xlsx")))?;
        Renderer::new(&data).write_tiff(&dir.join(format!("{stem}_processed.tiff")))?;
        eprintln!("wrote outputs to {}", dir.display());
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("run") => run(&args[1..]),
        Some("atlas") => {
            let t0 = Instant::now();
            let a = Atlas::load(&PathBuf::from(args.get(1).context(USAGE)?))?;
            println!("atlas {}x{}x{}, {} structures, loaded in {:.2}s", a.h, a.w, a.d, a.structures.len(), t0.elapsed().as_secs_f64());
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}
