use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
};

pub trait Task: Send + 'static {
    fn run(self);
}

pub(crate) struct ThreadPool<J: Task> {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<J>>,
}

impl<J: Task> ThreadPool<J> {
    pub fn new(size: usize) -> Self {
        assert!(size > 0);
        let (sender, receiver) = mpsc::channel::<J>();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(size);

        for _ in 0..size {
            workers.push(Worker::new(Arc::clone(&receiver)));
        }

        Self {
            workers,
            sender: Some(sender),
        }
    }

    #[inline]
    pub fn execute(&self, job: J) {
        self.sender.as_ref().unwrap().send(job).unwrap();
    }
}

impl<J: Task> Drop for ThreadPool<J> {
    fn drop(&mut self) {
        drop(self.sender.take()); // closes channel; workers exit
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
    fn new<J: Task>(receiver: Arc<Mutex<mpsc::Receiver<J>>>) -> Self {
        let thread = thread::spawn(move || {
            loop {
                let msg = {
                    let rx = receiver.lock().unwrap();
                    rx.recv()
                };
                match msg {
                    Ok(job) => job.run(),
                    Err(_) => break, // sender dropped
                }
            }
        });
        Self {
            thread: Some(thread),
        }
    }
}
