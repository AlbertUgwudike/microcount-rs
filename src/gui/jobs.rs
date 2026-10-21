//! Background work. Jobs run on their own threads with snapshots of the data they need
//! and report back through a channel that the UI drains every frame, so the workspace is
//! only ever mutated on the UI thread.

use crate::core::convert::Metadata;
use crate::core::image::Image;
use crate::core::pipeline::{NormCache, Preview, RegionOutcome};
use crate::core::registration::Affine;
use crate::core::Cancel;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

pub enum Msg {
    AtlasLoaded(Result<Arc<crate::core::atlas::Atlas>, String>),
    ConvertProgress { id: String, pct: f32 },
    ConvertDone { id: String, result: Result<Metadata, String> },
    ProcessStarted { rid: String },
    ProcessDone { rid: String, result: Result<RegionOutcome, String> },
    Registered { id: String, result: Result<Affine, String> },
    Preview { req: u64, result: Result<(Preview, NormCache), String> },
    /// Pixels for an image viewer, identified by a request number.
    Pixels { req: u64, result: Result<Vec<[Image<u16>; 3]>, String> },
    Done(String),
    Failed(String),
}

/// A pool of `max` workers draining a queue.
pub struct WorkQueue<T: Send + 'static> {
    queue: Arc<Mutex<VecDeque<T>>>,
    active: Arc<AtomicUsize>,
    max: usize,
}

impl<T: Send + 'static> WorkQueue<T> {
    pub fn new(max: usize) -> Self {
        Self { queue: Arc::default(), active: Arc::default(), max }
    }

    pub fn push(&self, items: Vec<T>, handler: Arc<dyn Fn(T) + Send + Sync>) {
        let mut q = self.queue.lock().unwrap();
        q.extend(items);
        while self.active.load(Ordering::SeqCst) < self.max.min(q.len()) {
            self.active.fetch_add(1, Ordering::SeqCst);
            let (queue, active, handler) = (self.queue.clone(), self.active.clone(), handler.clone());
            std::thread::spawn(move || loop {
                let item = {
                    let mut q = queue.lock().unwrap();
                    match q.pop_front() {
                        Some(i) => i,
                        None => {
                            active.fetch_sub(1, Ordering::SeqCst);
                            return;
                        }
                    }
                };
                handler(item);
            });
        }
    }

    /// Removes and returns everything still waiting.
    pub fn clear(&self) -> Vec<T> {
        self.queue.lock().unwrap().drain(..).collect()
    }

    pub fn busy(&self) -> bool {
        self.active.load(Ordering::SeqCst) > 0
    }
}

pub struct ConvertTask {
    pub id: String,
    pub source: std::path::PathBuf,
    pub series: usize,
    pub conv: std::path::PathBuf,
    pub down: std::path::PathBuf,
    pub cancel: Cancel,
}

pub struct ProcessTask {
    pub rid: String,
    pub run: Box<dyn FnOnce(&Cancel) -> anyhow::Result<RegionOutcome> + Send>,
    pub cancel: Cancel,
}

pub struct Jobs {
    pub tx: Sender<Msg>,
    pub rx: Receiver<Msg>,
    pub ctx: egui::Context,
    pub convert: WorkQueue<ConvertTask>,
    pub convert_cancel: Cancel,
    pub process: WorkQueue<ProcessTask>,
    pub process_cancel: Cancel,
    next_req: u64,
}

pub fn err_string(e: &anyhow::Error) -> String {
    if e.downcast_ref::<crate::core::Cancelled>().is_some() {
        "cancelled".to_string()
    } else {
        format!("{e:#}")
    }
}

impl Jobs {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        Self {
            tx,
            rx,
            ctx,
            convert: WorkQueue::new(2),
            convert_cancel: Cancel::default(),
            process: WorkQueue::new(1),
            process_cancel: Cancel::default(),
            next_req: 0,
        }
    }

    pub fn request_id(&mut self) -> u64 {
        self.next_req += 1;
        self.next_req
    }

    /// Runs `f` on a new thread and delivers its message.
    pub fn spawn(&self, f: impl FnOnce() -> Msg + Send + 'static) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            tx.send(f()).ok();
            ctx.request_repaint();
        });
    }

    pub fn convert(&mut self, tasks: Vec<ConvertTask>) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.convert.push(
            tasks,
            Arc::new(move |t: ConvertTask| {
                let id = t.id.clone();
                let progress = {
                    let (tx, ctx, id) = (tx.clone(), ctx.clone(), id.clone());
                    move |pct: f32| {
                        tx.send(Msg::ConvertProgress { id: id.clone(), pct }).ok();
                        ctx.request_repaint();
                    }
                };
                let result = if t.cancel.is_cancelled() {
                    Err("cancelled".into())
                } else {
                    log::info!("converting {}", t.source.display());
                    crate::core::convert::import(&t.source, t.series, &t.conv, &t.down, &t.cancel, &progress)
                        .map_err(|e| err_string(&e))
                };
                tx.send(Msg::ConvertDone { id, result }).ok();
                ctx.request_repaint();
            }),
        );
    }

    /// A fresh cancellation token for a new batch (the previous one may be cancelled).
    pub fn convert_token(&mut self) -> Cancel {
        if self.convert_cancel.is_cancelled() && !self.convert.busy() {
            self.convert_cancel = Cancel::default();
        }
        self.convert_cancel.clone()
    }

    pub fn process_token(&mut self) -> Cancel {
        if self.process_cancel.is_cancelled() && !self.process.busy() {
            self.process_cancel = Cancel::default();
        }
        self.process_cancel.clone()
    }

    pub fn process(&mut self, tasks: Vec<ProcessTask>) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.process.push(
            tasks,
            Arc::new(move |t: ProcessTask| {
                let result = if t.cancel.is_cancelled() {
                    Err("cancelled".into())
                } else {
                    tx.send(Msg::ProcessStarted { rid: t.rid.clone() }).ok();
                    ctx.request_repaint();
                    (t.run)(&t.cancel).map_err(|e| err_string(&e))
                };
                tx.send(Msg::ProcessDone { rid: t.rid, result }).ok();
                ctx.request_repaint();
            }),
        );
    }
}
