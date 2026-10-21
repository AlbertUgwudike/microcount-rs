fn main() {
    for p in std::env::args().skip(1) {
        match microcount::core::tiff::TiffFile::open(&p) {
            Ok(t) => {
                let pg = &t.pages[0];
                println!("{p}\n  pages {} {}x{} spp {} bps {} comp {} planar {} tiled {} px {:?}", t.pages.len(), pg.width, pg.height, pg.spp, pg.bps, pg.compression, pg.planar, pg.is_tiled(), t.pixel_size_um());
            }
            Err(e) => println!("{p}: {e:#}"),
        }
    }
}
