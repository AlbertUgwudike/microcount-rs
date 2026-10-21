//! egui front end. The five tabs mirror the MATLAB app: Home, Select Images, Register,
//! Select Regions and Analyse.

pub mod jobs;
pub mod log;
mod tabs;
pub mod widgets;

use crate::core::atlas::Atlas;
use crate::core::workspace::{ConvertStatus, ProcessStatus, Workspace};
use jobs::{Jobs, Msg};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Home,
    Select,
    Register,
    Regions,
    Analyse,
}

pub struct App {
    pub ws: Option<Workspace>,
    pub atlas: Option<Arc<Atlas>>,
    pub atlas_status: String,
    pub assets: Option<PathBuf>,
    /// Formats the bioformats crate can import.
    pub formats: Vec<&'static str>,
    pub tab: Tab,
    pub jobs: Jobs,
    log: log::LogBuffer,
    show_log: bool,
    errors: Vec<String>,
    logo: Option<egui::TextureHandle>,
    select: tabs::select::State,
    register: tabs::register::State,
    regions: tabs::regions::State,
    analyse: tabs::analyse::State,
    dirty: bool,
    shots: Option<Shots>,
}

/// Test hook: `MICROCOUNT_SCREENSHOTS=<dir>` (with `MICROCOUNT_WS=<workspace>`) visits
/// every tab with the first item selected, saves a PNG of each and quits.
struct Shots {
    dir: PathBuf,
    step: usize,
    frame: u32,
    waiting: bool,
}

/// Directories searched for `assets/` (atlas volumes).
fn search_roots() -> Vec<PathBuf> {
    let mut roots = vec![];
    if let Ok(p) = std::env::var("MICROCOUNT_HOME") {
        roots.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut d = exe.parent().map(Path::to_path_buf);
        for _ in 0..4 {
            if let Some(dir) = d {
                roots.push(dir.join("../Resources")); // macOS bundle
                roots.push(dir.clone());
                d = dir.parent().map(Path::to_path_buf);
            } else {
                break;
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.clone());
        roots.push(cwd.join(".."));
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    roots
}

fn find_assets() -> Option<PathBuf> {
    search_roots().into_iter().map(|r| r.join("assets")).find(|a| a.join("annotation.tiff").is_file())
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, log: log::LogBuffer) -> Self {
        log::set_context(&cc.egui_ctx);
        let jobs = Jobs::new(cc.egui_ctx.clone());
        let assets = find_assets();
        let atlas_status = match &assets {
            Some(dir) => {
                let dir = dir.clone();
                jobs.spawn(move || Msg::AtlasLoaded(Atlas::load(&dir).map(Arc::new).map_err(|e| format!("{e:#}"))));
                "loading…".into()
            }
            None => "atlas files not found (set MICROCOUNT_HOME to the folder containing assets/)".into(),
        };
        let logo = assets.as_ref().and_then(|a| {
            let img = image::open(a.join("microcount_logo.png")).ok()?.to_rgba8();
            let size = [img.width() as usize, img.height() as usize];
            let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
            Some(cc.egui_ctx.load_texture("logo", ci, egui::TextureOptions::LINEAR))
        });
        Self {
            ws: None,
            atlas: None,
            atlas_status,
            assets,
            formats: crate::core::imageio::readable_formats(),
            tab: Tab::Home,
            jobs,
            log,
            show_log: false,
            errors: vec![],
            logo,
            select: Default::default(),
            register: Default::default(),
            regions: Default::default(),
            analyse: Default::default(),
            dirty: false,
            shots: std::env::var("MICROCOUNT_SCREENSHOTS").ok().map(|d| Shots { dir: d.into(), step: 0, frame: 0, waiting: false }),
        }
    }

    fn drive_screenshots(&mut self, ctx: &egui::Context) {
        let Some(sh) = self.shots.as_mut() else { return };
        const TABS: [Tab; 5] = [Tab::Home, Tab::Select, Tab::Register, Tab::Regions, Tab::Analyse];
        ctx.request_repaint();
        for e in ctx.input(|i| i.raw.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = e {
                let path = sh.dir.join(format!("{}_{:?}.png", sh.step, TABS[sh.step] as u8));
                let [w, h] = image.size;
                let raw: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(buf) = ::image::RgbaImage::from_raw(w as u32, h as u32, raw) {
                    buf.save(&path).ok();
                }
                ::log::info!("screenshot {}", path.display());
                sh.waiting = false;
                sh.step += 1;
                sh.frame = 0;
            }
        }
        if sh.step >= TABS.len() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if sh.frame == 0 {
            if self.ws.is_none() {
                if let Ok(dir) = std::env::var("MICROCOUNT_WS") {
                    if let Ok(ws) = Workspace::load(Path::new(&dir)) {
                        self.ws = Some(ws);
                    }
                }
            }
            self.tab = TABS[sh.step];
            if let Some(ws) = &self.ws {
                if let Some(img) = ws.images.first() {
                    self.select.sel.focus_on(&img.id);
                    self.register.sel.focus_on(&img.id);
                    self.regions.sel.focus_on(&img.id);
                    if let Some(r) = img.regions.first() {
                        self.analyse.sel.focus_on(&r.id(&img.id));
                    }
                }
            }
        }
        sh.frame += 1;
        if sh.frame == 90 && !sh.waiting && self.atlas.is_some() {
            sh.waiting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        } else if sh.frame > 90 && !sh.waiting {
            sh.frame = 89;
        }
    }

    pub fn error(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        ::log::error!("{msg}");
        self.errors.push(msg);
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn open_workspace(&mut self, ws: Workspace) {
        ::log::info!("workspace {}", ws.dir.display());
        self.ws = Some(ws);
        self.select = Default::default();
        self.register = Default::default();
        self.regions = Default::default();
        self.analyse = Default::default();
        self.tab = Tab::Select;
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::AtlasLoaded(Ok(a)) => {
                self.atlas_status = format!("{}×{}×{} voxels, {} structures", a.h, a.w, a.d, a.structures.len());
                self.atlas = Some(a);
            }
            Msg::AtlasLoaded(Err(e)) => {
                self.atlas_status = format!("failed: {e}");
                self.error(format!("Could not load the atlas: {e}"));
            }
            Msg::ConvertProgress { id, pct } => {
                if let Some(img) = self.ws.as_mut().and_then(|w| w.image_mut(&id)) {
                    img.progress = pct;
                }
            }
            Msg::ConvertDone { id, result } => {
                let Some(ws) = self.ws.as_mut() else { return };
                let Some(img) = ws.image_mut(&id) else { return };
                match result {
                    Ok(md) => {
                        img.size = md.size;
                        img.down_size = md.down_size;
                        img.channel_count = md.channel_count;
                        img.set_default_channels();
                        if let Some(px) = md.pixel_dims {
                            img.pixel_dims = px;
                        }
                        img.status = ConvertStatus::Converted;
                        img.progress = 100.0;
                        ::log::info!("{id}: converted ({}×{}, {} channels)", md.size[1], md.size[0], md.channel_count);
                    }
                    Err(e) => {
                        img.status = ConvertStatus::Unconverted;
                        img.progress = 0.0;
                        if e != "cancelled" {
                            self.error(format!("{id}: {e}"));
                        }
                    }
                }
                self.dirty = true;
            }
            Msg::ProcessStarted { rid } => {
                if let Some(r) = self.region_mut(&rid) {
                    r.status = ProcessStatus::Processing;
                }
            }
            Msg::ProcessDone { rid, result } => {
                match result {
                    Ok(out) => {
                        ::log::info!("{rid}: processed in {:.1}s, {} cells", out.seconds, out.result.cell_count);
                        if let Some(r) = self.region_mut(&rid) {
                            r.status = ProcessStatus::Processed;
                            r.result = Some(out.result);
                            r.result_out_plaque = out.result_out_plaque;
                            r.crop = Some(out.crop);
                        }
                        self.analyse.invalidate_view(&rid);
                    }
                    Err(e) => {
                        if let Some(r) = self.region_mut(&rid) {
                            r.status = if r.result.is_some() { ProcessStatus::Processed } else { ProcessStatus::Unprocessed };
                        }
                        if e != "cancelled" {
                            self.error(format!("{rid}: {e}"));
                        }
                    }
                }
                self.dirty = true;
            }
            Msg::Registered { id, result } => tabs::register::on_registered(self, id, result),
            Msg::Preview { req, result } => tabs::analyse::on_preview(self, req, result),
            Msg::Pixels { req, result } => {
                if !tabs::select::on_pixels(self, req, &result) {
                    tabs::analyse::on_pixels(self, req, result);
                }
            }
            Msg::Done(m) => ::log::info!("{m}"),
            Msg::Failed(m) => self.error(m),
        }
    }

    pub fn region_mut(&mut self, rid: &str) -> Option<&mut crate::core::workspace::Region> {
        let ws = self.ws.as_mut()?;
        let (ii, ri) = ws.find_region(rid)?;
        Some(&mut ws.images[ii].regions[ri])
    }

    fn save(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if let Some(ws) = &self.ws {
            if let Err(e) = ws.save() {
                self.error(format!("Could not save the workspace: {e:#}"));
            }
        }
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let has_ws = self.ws.is_some();
            for (tab, label) in [
                (Tab::Home, "Home"),
                (Tab::Select, "Select Images"),
                (Tab::Register, "Register"),
                (Tab::Regions, "Select Regions"),
                (Tab::Analyse, "Analyse"),
            ] {
                let enabled = has_ws || tab == Tab::Home;
                if ui.add_enabled(enabled, egui::SelectableLabel::new(self.tab == tab, label)).clicked() {
                    self.tab = tab;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.toggle_value(&mut self.show_log, "Log");
                let converting = self.jobs.convert.busy();
                let processing = self.jobs.process.busy();
                if converting || processing {
                    ui.spinner();
                    ui.label(match (converting, processing) {
                        (true, true) => "converting and processing…",
                        (true, false) => "converting…",
                        _ => "processing…",
                    });
                }
            });
        });
    }

    fn log_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("log").resizable(true).default_height(160.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Log");
                if ui.small_button("Clear").clicked() {
                    self.log.clear();
                }
            });
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false; 2]).show(ui, |ui| {
                for line in self.log.lines() {
                    let colour = match line.level {
                        ::log::Level::Error => egui::Color32::from_rgb(230, 90, 80),
                        ::log::Level::Warn => egui::Color32::from_rgb(230, 180, 60),
                        _ => ui.visuals().text_color(),
                    };
                    ui.label(egui::RichText::new(line.text).monospace().color(colour));
                }
            });
        });
    }

    fn error_windows(&mut self, ctx: &egui::Context) {
        if let Some(msg) = self.errors.first().cloned() {
            let mut open = true;
            egui::Window::new("Error")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.set_max_width(480.0);
                    ui.label(&msg);
                    if ui.button("OK").clicked() {
                        self.errors.remove(0);
                    }
                });
            if !open && !self.errors.is_empty() {
                self.errors.remove(0);
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(msg) = self.jobs.rx.try_recv() {
            self.handle(msg);
        }
        self.drive_screenshots(ctx);
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| self.tab_bar(ui));
        if self.show_log {
            self.log_panel(ctx);
        }
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Home => tabs::home::ui(self, ui),
            Tab::Select => {
                let mut s = std::mem::take(&mut self.select);
                tabs::select::ui(self, ui, &mut s);
                self.select = s;
            }
            Tab::Register => {
                let mut s = std::mem::take(&mut self.register);
                tabs::register::ui(self, ui, &mut s);
                self.register = s;
            }
            Tab::Regions => {
                let mut s = std::mem::take(&mut self.regions);
                tabs::regions::ui(self, ui, &mut s);
                self.regions = s;
            }
            Tab::Analyse => {
                let mut s = std::mem::take(&mut self.analyse);
                tabs::analyse::ui(self, ui, &mut s);
                self.analyse = s;
            }
        });
        self.error_windows(ctx);
        self.save();
        if self.jobs.convert.busy() || self.jobs.process.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }
}

pub fn run() -> eframe::Result<()> {
    let log = log::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Microcount").with_inner_size([1360.0, 820.0]).with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native("Microcount", options, Box::new(move |cc| Ok(Box::new(App::new(cc, log)))))
}
