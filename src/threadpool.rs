use std::{sync::mpsc, thread};

pub trait Task: Send + 'static {
    fn run(self);
}

pub(crate) struct ThreadPool<J: Task> {
    workers: Vec<Worker>,
    senders: Vec<mpsc::Sender<J>>,
}

impl<J: Task> ThreadPool<J> {
    pub fn new(size: usize) -> Self {
        assert!(size > 0);

        let mut workers = Vec::with_capacity(size);
        let mut senders = Vec::with_capacity(size);

        for _ in 0..size {
            let (tx, rx) = mpsc::channel::<J>();
            senders.push(tx);
            workers.push(Worker::new(rx));
        }

        Self { workers, senders }
    }

    #[inline]
    pub(crate) fn execute_keyed(&self, job: J, key: usize) {
        // TODO: how to add low-cost load-awareness?
        let i = key % self.senders.len();
        self.senders[i].send(job).unwrap();
    }
}

impl<J: Task> Drop for ThreadPool<J> {
    fn drop(&mut self) {
        // Drop all senders so worker recv()s return Err and threads exit.
        self.senders.clear();
        for w in &mut self.workers {
            if let Some(t) = w.thread.take() {
                t.join().unwrap();
            }
        }
    }
}

struct Worker {
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new<J: Task>(rx: mpsc::Receiver<J>) -> Self {
        let thread = thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                job.run();
            }
            // channel closed => exit
        });
        Self {
            thread: Some(thread),
        }
    }
}
