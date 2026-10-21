//! Logger that mirrors messages to stderr and keeps the recent ones for the log panel.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone)]
pub struct Line {
    pub level: log::Level,
    pub text: String,
}

#[derive(Clone, Default)]
pub struct LogBuffer(Arc<Mutex<VecDeque<Line>>>);

impl LogBuffer {
    pub fn lines(&self) -> Vec<Line> {
        self.0.lock().unwrap().iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

struct Logger {
    buf: LogBuffer,
    ctx: OnceLock<egui::Context>,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.target().starts_with("microcount") && m.level() <= log::Level::Debug
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let text = format!("{}", r.args());
        eprintln!("[{}] {text}", r.level());
        if r.level() <= log::Level::Info {
            let mut b = self.buf.0.lock().unwrap();
            b.push_back(Line { level: r.level(), text });
            while b.len() > 2000 {
                b.pop_front();
            }
            if let Some(ctx) = self.ctx.get() {
                ctx.request_repaint();
            }
        }
    }

    fn flush(&self) {}
}

pub fn init() -> LogBuffer {
    let l = LOGGER.get_or_init(|| Logger { buf: LogBuffer::default(), ctx: OnceLock::new() });
    if log::set_logger(l).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
    l.buf.clone()
}

pub fn set_context(ctx: &egui::Context) {
    if let Some(l) = LOGGER.get() {
        l.ctx.set(ctx.clone()).ok();
    }
}
