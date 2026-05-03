use std::fmt::Debug;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, future::Future};

use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;

type Job<K> = Pin<Box<dyn Future<Output = K> + Send>>;

pub struct ThreadPool<T, K> {
    pub sender: mpsc::Sender<(Job<K>, Option<T>)>,
    pub reciever: mpsc::Receiver<K>,
}

impl<T: Send + 'static + Eq + Hash + Copy + Debug, K: Send + 'static> ThreadPool<T, K> {
    pub fn new(worker_limit: usize, queue_size: usize) -> Self {
        let (tx, mut rx) = mpsc::channel::<(Job<K>, Option<T>)>(queue_size);
        let (ttx, rrx) = mpsc::channel::<K>(queue_size);

        let semaphore = Arc::new(Semaphore::new(worker_limit));
        let hm: HashMap<T, CancellationToken> = HashMap::new();
        let cache = Arc::new(Mutex::new(hm));
        let ttx = Arc::new(ttx);

        // dispatcher task
        tokio::spawn({
            let semaphore = Arc::clone(&semaphore);
            let hm = Arc::clone(&cache);
            println!("Job receieved!!");

            async move {
                while let Some((job, label)) = rx.recv().await {
                    let permit = semaphore.clone();
                    let ttx = Arc::clone(&ttx);

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
                                    ttx.send(job.await).await.unwrap();
                                    println!("Job Completed");
                                } => {}
                            }
                        });
                    } else {
                        ttx.send(job.await);
                    }
                }
            }
        });

        Self {
            sender: tx,
            reciever: rrx,
        }
    }

    pub fn dispatch<F>(&self, job: F)
    where
        F: Future<Output = K> + Send + 'static,
    {
        println!("Attempting send!");
        let _ = self.sender.send((Box::pin(job), None));
    }

    pub fn dispatch_exclusive<F>(&self, job: F, label: T)
    where
        F: Future<Output = K> + Send + 'static,
    {
        let _ = self.sender.try_send((Box::pin(job), Some(label)));
    }
}
