//! End-to-end workflow without the GUI: workspace -> import -> register -> region ->
//! process -> export.
use microcount::core::analysis::Algorithm;
use microcount::core::atlas::Atlas;
use microcount::core::convert::{self, Converter};
use microcount::core::registration::auto_register;
use microcount::core::workspace::*;
use microcount::core::{pipeline, Cancel};
use std::path::PathBuf;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (src, wsdir, region) = (PathBuf::from(&args[1]), PathBuf::from(&args[2]), args.get(3).cloned().unwrap_or("HPF".into()));
    let t = Instant::now();
    std::fs::remove_dir_all(&wsdir).ok();
    let mut ws = Workspace::create(&wsdir)?;
    ws.add_images(&[src.clone()]);
    let paths = ws.paths();
    let atlas = Atlas::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"))?;
    let conv = Converter { java: convert::find_java(), jar: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("convert/convert.jar"), heap: "8g".into() };
    let img = &mut ws.images[0];
    let md = convert::import(&conv, &src, &paths.conv_path(&img.id), &paths.down_path(&img.id), &Cancel::default(), &|p| eprint!("\r{p:.0}%  "))?;
    println!("imported {:?} down {:?} ch {} px {:?} [{:.1}s]", md.size, md.down_size, md.channel_count, md.pixel_dims, t.elapsed().as_secs_f64());
    img.status = ConvertStatus::Converted; img.size = md.size; img.down_size = md.down_size; img.channel_count = md.channel_count; img.set_default_channels();
    img.pixel_dims = md.pixel_dims.unwrap_or([1.0, 1.0]);
    let ori = Orientation::Axial;
    let mut td = TransformData::default_for(img.down_size, atlas.slice_size(ori), atlas.n_slices(ori), Direction::North, ori);
    td.slice_idx = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(td.slice_idx);
    let tf = microcount::core::tiff::TiffFile::open(paths.down_path(&img.id))?;
    let hist = tf.read_channel(img.registration_channel - 1, microcount::core::image::Rect::new(0, 0, img.down_size[1], img.down_size[0]))?.pad(PAD, 0);
    let t1 = Instant::now();
    td.matrix = auto_register(&atlas.reference_slice(ori, td.slice_idx), &hist)?.0;
    println!("registered in {:.2}s: {:?}", t1.elapsed().as_secs_f64(), td.matrix);
    img.transform = Some(td);
    img.aligned = true;
    let mut r = Region::new(&region, Laterality::Left);
    r.dn_rect = Some(pipeline::write_mask_thumbnail(&paths, &atlas, img, &r)?);
    img.regions.push(r.clone());
    let t2 = Instant::now();
    let out = pipeline::process_region(&paths, &atlas, img, &r, Algorithm::MicroFluor, &Cancel::default())?;
    println!("processed {} in {:.1}s: {} cells, crop {:?}", r.id(&img.id), t2.elapsed().as_secs_f64(), out.result.cell_count, out.crop);
    let rr = &mut img.regions[0];
    rr.result = Some(out.result); rr.crop = Some(out.crop); rr.status = ProcessStatus::Processed;
    ws.save()?;
    let ws = Workspace::load(&wsdir)?;
    let rows: Vec<_> = ws.images.iter().flat_map(|i| i.regions.iter().map(move |r| (i, r))).collect();
    pipeline::export_csv(&ws.paths(), &rows, &wsdir.join("data.csv"))?;
    println!("{}", std::fs::read_to_string(wsdir.join("data.csv"))?);
    for d in ["ws_converted", "ws_downsampled", "ws_masks", "ws_processed", "ws_scholl"] {
        for e in std::fs::read_dir(wsdir.join(d))? { let e = e?; println!("  {d}/{} {} MB", e.file_name().to_string_lossy(), e.metadata()?.len() / 1_000_000); }
    }
    println!("total {:.1}s", t.elapsed().as_secs_f64());
    Ok(())
}
