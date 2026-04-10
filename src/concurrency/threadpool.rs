use std::{
    future::Future,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex, MutexGuard,
    },
    thread::{self, JoinHandle},
};

// type Job = Box<dyn FnOnce() + Send + 'static>;
type Job = Box<dyn Future<Output = ()> + Send + 'static>;

pub struct ThreadPool<T> {
    tx: Sender<(Option<T>, Job)>,
    rx: Arc<Mutex<Receiver<(Option<T>, Job)>>>,
    workers: Vec<Worker<T>>,
}

impl<T: Copy + Send + PartialEq + 'static> ThreadPool<T> {
    pub fn new() -> Self {
        let (tx, rx) = channel::<(Option<T>, Job)>();
        let rx = Arc::new(Mutex::new(rx));
        let workers = (0..1).map(|i| Worker::new(i, Arc::clone(&rx))).collect();
        ThreadPool { tx, rx, workers }
    }

    pub fn reset(&mut self, label: T) {
        let mut idxs = vec![];
        for (i, worker) in self.workers.iter_mut().enumerate() {
            let option_mg = worker.label.try_lock().ok();
            if let Some(mg) = option_mg {
                if *mg == Some(label) {
                    println!("Worker {} replaced!", worker.id);
                    let id = worker.thread_handle.thread().id();
                    idxs.push(i);
                }
            };
        }

        for idx in idxs {
            self.workers[idx] = Worker::new(self.workers[idx].id, Arc::clone(&self.rx));
        }
        // self.workers
        //     .iter_mut()
        //     .filter(|w| *w.label.try_lock().unwrap_or(None.into()) == Some(label))
        //     .for_each(|w| {
        //         println!("Worker {} replaced!", w.id);
        //         w.thread_handle.abort();
        //         // println!("Worker {} replaced!", w.thread_handle.id());
        //         *w = Worker::new(w.id, Arc::clone(&self.rx))
        //     });
    }

    pub fn dispatch<F>(&self, f: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.tx.send((None, Box::new(f))).unwrap();
    }

    pub fn dispatch_exclusive<F>(&mut self, label: T, f: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.reset(label);
        self.tx.send((Some(label), Box::new(f))).unwrap();
    }
}

struct Worker<T> {
    id: i32,
    label: Arc<Mutex<Option<T>>>,
    thread_handle: JoinHandle<()>,
}

impl<T: Copy + Send + 'static> Worker<T> {
    pub fn new(id: i32, r: Arc<Mutex<Receiver<(Option<T>, Job)>>>) -> Self {
        let w_label = Arc::new(Mutex::new(None));
        let t_label = Arc::clone(&w_label);

        let thread_handle = tokio::spawn(async {
            loop {
                let (label, job) = r.lock().unwrap().recv().unwrap();
                println!("Worker {} received msg.", id);
                *t_label.lock().unwrap() = label;
                // std::pin::pin!(job);
                job.await;
                println!("Worker {} completed Job.", id);
            }
        });

        Worker {
            id,
            label: w_label,
            thread_handle,
        }
    }
}
