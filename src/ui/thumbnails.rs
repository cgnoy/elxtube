use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};

use crate::application::services::{ThumbnailAsset, ThumbnailService};

type ThumbnailCallback = Rc<dyn Fn(u64, u64, Result<ThumbnailAsset, String>)>;
type ThumbnailQueue = Arc<(Mutex<VecDeque<(u8, u64, ThumbnailRequest)>>, Condvar)>;

const MAX_PENDING_THUMBNAILS: usize = 256;

#[derive(Clone)]
pub struct ThumbnailPipeline {
    queue: ThumbnailQueue,
    callbacks: Rc<RefCell<HashMap<u64, ThumbnailCallback>>>,
    next_id: Rc<Cell<u64>>,
    epoch: Rc<Cell<u64>>,
    pending: Arc<AtomicUsize>,
    active: Arc<Mutex<HashMap<String, ActiveThumbnail>>>,
    receiver: Arc<Mutex<mpsc::Receiver<ThumbnailResult>>>,
    sender: mpsc::Sender<ThumbnailResult>,
    writer: mpsc::Sender<Vec<(String, Arc<[u8]>)>>,
    thumbnail: ThumbnailService,
    workers_started: Rc<Cell<bool>>,
    dispatcher_started: Rc<Cell<bool>>,
}

struct ActiveThumbnail {
    generation: u64,
    epoch: u64,
    cancelled: Arc<AtomicBool>,
}

struct ThumbnailRequest {
    id: u64,
    key: String,
    url: String,
    generation: u64,
    epoch: u64,
    dimensions: Option<(u32, u32)>,
    priority: u8,
    cancelled: Arc<AtomicBool>,
}

struct ThumbnailResult {
    id: u64,
    key: String,
    generation: u64,
    result: Result<ThumbnailAsset, String>,
}

impl ThumbnailPipeline {
    pub fn new(thumbnail: ThumbnailService) -> Self {
        let queue = Arc::new((
            Mutex::new(VecDeque::<(u8, u64, ThumbnailRequest)>::new()),
            Condvar::new(),
        ));
        let (sender, receiver) = mpsc::channel::<ThumbnailResult>();
        let callbacks = Rc::new(RefCell::new(HashMap::new()));
        let next_id = Rc::new(Cell::new(1));
        let epoch = Rc::new(Cell::new(0));
        let pending = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(Mutex::new(HashMap::<String, ActiveThumbnail>::new()));

        let (writer, writes) = mpsc::channel::<Vec<(String, Arc<[u8]>)>>();
        let writer_service = thumbnail.clone();
        std::thread::spawn(move || {
            while let Ok(batch) = writes.recv() {
                let _ = writer_service.put_thumbnails(&batch);
            }
        });

        Self {
            queue,
            callbacks,
            next_id,
            epoch,
            pending,
            active,
            receiver: Arc::new(Mutex::new(receiver)),
            sender,
            writer,
            thumbnail,
            workers_started: Rc::new(Cell::new(false)),
            dispatcher_started: Rc::new(Cell::new(false)),
        }
    }

    fn ensure_workers(&self) {
        if self.workers_started.get() {
            return;
        }
        self.workers_started.set(true);
        for _ in 0..4 {
            let worker_queue = self.queue.clone();
            let worker_sender = self.sender.clone();
            let worker_pending = self.pending.clone();
            let worker_active = self.active.clone();
            let worker_thumbnail = self.thumbnail.clone();
            std::thread::spawn(move || loop {
                let request = {
                    let (lock, condvar) = &*worker_queue;
                    let mut pending = lock.lock().unwrap();
                    loop {
                        if let Some((_, _, request)) = pending.pop_front() {
                            break request;
                        }
                        pending = condvar.wait(pending).unwrap();
                    }
                };
                let result = if request.cancelled.load(Ordering::Acquire) {
                    Err("permintaan dibatalkan".to_string())
                } else {
                    worker_thumbnail
                        .fetch(&request.url, request.dimensions)
                        .map_err(|error| error.to_string())
                };
                let result = if request.cancelled.load(Ordering::Acquire) {
                    Err("permintaan dibatalkan".to_string())
                } else {
                    result
                };
                let sent = worker_sender
                    .send(ThumbnailResult {
                        id: request.id,
                        key: request.key.clone(),
                        generation: request.generation,
                        result,
                    })
                    .is_ok();
                let mut active = worker_active.lock().unwrap();
                if active
                    .get(&request.key)
                    .is_some_and(|current| current.generation == request.generation)
                {
                    active.remove(&request.key);
                }
                worker_pending.fetch_sub(1, Ordering::SeqCst);
                if !sent {
                    break;
                }
            });
        }
    }

    fn start_dispatcher(&self) {
        let callbacks = self.callbacks.clone();
        let pending = self.pending.clone();
        let dispatcher_started = self.dispatcher_started.clone();
        let receiver = self.receiver.clone();
        let writer = self.writer.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(25), move || {
            let mut batch = Vec::new();
            loop {
                let result = match receiver.lock() {
                    Ok(receiver) => receiver.try_recv(),
                    Err(_) => break,
                };
                let result = match result {
                    Ok(result) => result,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => break,
                };
                if let Ok(asset) = &result.result {
                    batch.push((result.key.clone(), asset.jpeg.clone()));
                }
                let callback = callbacks.borrow_mut().remove(&result.id);
                if let Some(callback) = callback {
                    callback(result.id, result.generation, result.result);
                }
            }
            if !batch.is_empty() && writer.send(batch).is_err() {
                eprintln!("[thumbnail] writer database berhenti");
            }
            if pending.load(Ordering::SeqCst) == 0 {
                dispatcher_started.set(false);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    pub fn cancel_stale(&self) {
        let next_epoch = self.epoch.get().wrapping_add(1);
        self.epoch.set(next_epoch);
        for active in self.active.lock().unwrap().values() {
            active.cancelled.store(true, Ordering::Release);
        }
        let mut cancelled_ids = Vec::new();
        {
            let (lock, _) = &*self.queue;
            let mut queue = lock.lock().unwrap();
            let mut remaining = VecDeque::with_capacity(queue.len());
            while let Some((priority, sequence, request)) = queue.pop_front() {
                if request.epoch < next_epoch {
                    request.cancelled.store(true, Ordering::Release);
                    cancelled_ids.push((request.id, request.generation));
                } else {
                    remaining.push_back((priority, sequence, request));
                }
            }
            *queue = remaining;
        }
        if !cancelled_ids.is_empty() {
            self.pending
                .fetch_sub(cancelled_ids.len(), Ordering::SeqCst);
            let mut callbacks = self.callbacks.borrow_mut();
            for (id, generation) in cancelled_ids {
                if let Some(callback) = callbacks.remove(&id) {
                    callback(id, generation, Err("thumbnail dibatalkan".to_string()));
                }
            }
        }
    }

    pub fn request<F>(&self, key: String, url: String, generation: u64, callback: F) -> u64
    where
        F: Fn(u64, u64, Result<ThumbnailAsset, String>) + 'static,
    {
        self.enqueue(key, url, generation, None, 0, callback)
    }

    pub fn request_sized<F>(
        &self,
        key: String,
        url: String,
        generation: u64,
        width: u32,
        height: u32,
        callback: F,
    ) -> u64
    where
        F: Fn(u64, u64, Result<ThumbnailAsset, String>) + 'static,
    {
        self.enqueue(key, url, generation, Some((width, height)), 1, callback)
    }

    pub fn request_with_priority<F>(
        &self,
        key: String,
        url: String,
        generation: u64,
        priority: u8,
        callback: F,
    ) -> u64
    where
        F: Fn(u64, u64, Result<ThumbnailAsset, String>) + 'static,
    {
        self.enqueue(key, url, generation, None, priority, callback)
    }

    fn enqueue<F>(
        &self,
        key: String,
        url: String,
        generation: u64,
        dimensions: Option<(u32, u32)>,
        priority: u8,
        callback: F,
    ) -> u64
    where
        F: Fn(u64, u64, Result<ThumbnailAsset, String>) + 'static,
    {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let callback = Rc::new(callback);
        if self.pending.load(Ordering::SeqCst) >= MAX_PENDING_THUMBNAILS {
            callback(id, generation, Err("thumbnail queue penuh".to_string()));
            return id;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let epoch = self.epoch.get();
        {
            let mut active = self.active.lock().unwrap();
            if let Some(current) = active.get(&key) {
                if current.generation >= generation && current.epoch >= epoch {
                    callback(id, generation, Err("thumbnail sudah diproses".to_string()));
                    return id;
                }
                current.cancelled.store(true, Ordering::Release);
            }
            active.insert(
                key.clone(),
                ActiveThumbnail {
                    generation,
                    epoch,
                    cancelled: cancelled.clone(),
                },
            );
        }
        self.callbacks.borrow_mut().insert(id, callback);
        self.pending.fetch_add(1, Ordering::SeqCst);
        self.ensure_workers();
        let sequence = self.next_id.get();
        let (lock, condvar) = &*self.queue;
        let mut pending = lock.lock().unwrap();
        let position = pending
            .iter()
            .position(|(_, _, request)| request.priority < priority)
            .unwrap_or(pending.len());
        pending.insert(
            position,
            (
                priority,
                sequence,
                ThumbnailRequest {
                    id,
                    key,
                    url,
                    generation,
                    epoch,
                    dimensions,
                    priority,
                    cancelled,
                },
            ),
        );
        condvar.notify_one();
        if !self.dispatcher_started.get() {
            self.dispatcher_started.set(true);
            self.start_dispatcher();
        }
        id
    }
}
