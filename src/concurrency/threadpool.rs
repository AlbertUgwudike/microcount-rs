use std::fmt::Debug;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, future::Future};

use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;

type Job = Pin<Box<dyn Future<Output = ()> + Send>>;

pub struct ThreadPool<T> {
    sender: mpsc::Sender<(Job, Option<T>)>,
}

impl<T: Send + 'static + Eq + Hash + Copy + Debug> ThreadPool<T> {
    pub fn new(worker_limit: usize, queue_size: usize) -> Self {
        let (tx, mut rx) = mpsc::channel::<(Job, Option<T>)>(queue_size);
        let semaphore = Arc::new(Semaphore::new(worker_limit));
        let hm: HashMap<T, CancellationToken> = HashMap::new();
        let cache = Arc::new(Mutex::new(hm));

        // dispatcher task
        tokio::spawn({
            let semaphore = Arc::clone(&semaphore);
            let hm = Arc::clone(&cache);
            println!("Job receieved!!");

            async move {
                while let Some((job, label)) = rx.recv().await {
                    let permit = semaphore.clone();

                    if let Some(l) = label {
                        let token = CancellationToken::new();

                        if let Some(ct) = hm.lock().unwrap().insert(l, token.clone()) {
                            ct.cancel();
                        }

                        tokio::spawn(async move {
                            tokio::select! {
                                _ = token.cancelled() => {
                                    println!("Job Cancelled");
                                }
                                _ = async {
                                    let _ = permit.acquire().await.unwrap();
                                    job.await;
                                    println!("Job Completed");
                                } => {}
                            }
                        });
                    } else {
                        job.await
                    }
                }
            }
        });

        Self { sender: tx }
    }

    pub fn dispatch<F>(&self, job: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        println!("Attempting send!");
        let _ = self.sender.send((Box::pin(job), None));
    }

    pub fn dispatch_exclusive<F>(&self, job: F, label: T)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let _ = self.sender.try_send((Box::pin(job), Some(label)));
    }
}
