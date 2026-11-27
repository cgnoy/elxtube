use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::i18n;
use gtk::prelude::*;
use gtk::Label;

use crate::application::services::WatchHistoryService;
use crate::core::model::Video;

enum HistoryCommand {
    RecordVideo {
        session_id: String,
        video: Video,
    },
    RecordProgress {
        session_id: String,
        video: Video,
        position: f64,
        watched_delta: f64,
    },
}

type HistoryResult = Result<(), String>;

#[derive(Clone)]
pub(super) struct HistoryState {
    writer: mpsc::Sender<HistoryCommand>,
    current_video: Rc<RefCell<Option<Video>>>,
    session_id: String,
    last_written: Rc<RefCell<Option<(String, f64)>>>,
    last_checkpoint: Rc<RefCell<Option<Instant>>>,
    last_sample: Rc<RefCell<Option<Instant>>>,
    watched_accumulator: Rc<RefCell<f64>>,
    writes_enabled: Cell<bool>,
    paused: Cell<bool>,
    status_label: Label,
}

impl HistoryState {
    pub(super) fn new(
        history: WatchHistoryService,
        current_video: Rc<RefCell<Option<Video>>>,
        session_id: String,
        status_label: Label,
    ) -> Self {
        let (writer, commands) = mpsc::channel::<HistoryCommand>();
        let results = Arc::new(Mutex::new(VecDeque::<HistoryResult>::new()));
        let worker_results = results.clone();
        let (notifier_reader, notifier_writer) =
            UnixStream::pair().expect("create history notification socket");
        let history_worker = history.clone();
        std::thread::spawn(move || {
            let mut notifier_writer = notifier_writer;
            while let Ok(command) = commands.recv() {
                let result = match command {
                    HistoryCommand::RecordVideo { session_id, video } => history_worker
                        .record_video(&session_id, &video)
                        .map_err(|error| error.to_string()),
                    HistoryCommand::RecordProgress {
                        session_id,
                        video,
                        position,
                        watched_delta,
                    } => history_worker
                        .record_progress(
                            &session_id,
                            &video,
                            position,
                            video.duration,
                            watched_delta,
                        )
                        .map_err(|error| error.to_string()),
                };
                worker_results.lock().unwrap().push_back(result);
                let _ = notifier_writer.write_all(&[1]);
            }
        });
        let notifier_stream = unsafe { gtk::gio::UnixInputStream::take_fd(notifier_reader) };
        let result_status = status_label.clone();
        let result_queue = results.clone();
        glib::MainContext::default().spawn_local(async move {
            loop {
                let read = notifier_stream
                    .read_future(vec![0u8; 64], glib::Priority::DEFAULT)
                    .await;
                match read {
                    Ok((_, 0)) | Err(_) => break,
                    Ok(_) => {}
                }
                let mut results = result_queue.lock().unwrap();
                for result in results.drain(..) {
                    if let Err(error) = result {
                        result_status
                            .set_text(&i18n::format("history_save_failed", &[("error", &error)]));
                        result_status.set_visible(true);
                    }
                }
            }
        });
        Self {
            writer,
            current_video,
            session_id,
            last_written: Rc::new(RefCell::new(None)),
            last_checkpoint: Rc::new(RefCell::new(None)),
            last_sample: Rc::new(RefCell::new(None)),
            watched_accumulator: Rc::new(RefCell::new(0.0)),
            writes_enabled: Cell::new(false),
            paused: Cell::new(false),
            status_label,
        }
    }

    pub(super) fn write(&self, position: f64, force: bool) {
        if !self.writes_enabled.get() {
            return;
        }
        let Some(video) = self.current_video.borrow().clone() else {
            return;
        };
        if position <= 5.0 {
            return;
        }

        let unchanged =
            self.last_written
                .borrow()
                .as_ref()
                .is_some_and(|(video_id, last_position)| {
                    video_id == &video.id && (position - last_position).abs() < 0.5
                });
        if unchanged && !force {
            return;
        }

        let watched_delta = self.watched_accumulator.replace(0.0);
        let command = HistoryCommand::RecordProgress {
            session_id: self.session_id.clone(),
            video: video.clone(),
            position,
            watched_delta,
        };
        if self.writer.send(command).is_ok() {
            *self.last_written.borrow_mut() = Some((video.id, position));
        } else {
            self.status_label
                .set_text(i18n::tr("history_writer_stopped"));
            self.status_label.set_visible(true);
        }
    }

    pub(super) fn flush(&self, position: f64) {
        self.accumulate_watched_time();
        self.write(position, true);
        *self.last_checkpoint.borrow_mut() = None;
    }

    pub(super) fn checkpoint(&self, position: f64, paused: bool) {
        self.accumulate_watched_time();
        self.paused.set(paused);
        let due = {
            let mut checkpoint = self.last_checkpoint.borrow_mut();
            match *checkpoint {
                Some(last) if last.elapsed() < Duration::from_secs(5) => false,
                Some(_) => {
                    *checkpoint = Some(Instant::now());
                    true
                }
                None => {
                    *checkpoint = Some(Instant::now());
                    true
                }
            }
        };
        if due {
            self.write(position, false);
        }
    }

    fn accumulate_watched_time(&self) {
        let now = Instant::now();
        let mut last_sample = self.last_sample.borrow_mut();
        if let Some(previous) = *last_sample {
            if !self.paused.get() {
                *self.watched_accumulator.borrow_mut() +=
                    now.duration_since(previous).as_secs_f64();
            }
        }
        *last_sample = Some(now);
    }

    pub(super) fn set_paused(&self, paused: bool) {
        self.accumulate_watched_time();
        self.paused.set(paused);
    }

    pub(super) fn activate(&self) {
        self.writes_enabled.set(true);
    }

    pub(super) fn video_changed(&self, video: &Video) {
        let changed = self
            .last_written
            .borrow()
            .as_ref()
            .map_or(true, |(last_video_id, _)| last_video_id != &video.id);
        if changed {
            *self.last_written.borrow_mut() = None;
            *self.last_checkpoint.borrow_mut() = None;
            *self.watched_accumulator.borrow_mut() = 0.0;
        }
        let command = HistoryCommand::RecordVideo {
            session_id: self.session_id.clone(),
            video: video.clone(),
        };
        if self.writer.send(command).is_err() {
            self.status_label
                .set_text(i18n::tr("history_writer_stopped"));
            self.status_label.set_visible(true);
        }
    }

    pub(super) fn reset(&self) {
        self.writes_enabled.set(false);
        self.paused.set(false);
        *self.last_written.borrow_mut() = None;
        *self.last_checkpoint.borrow_mut() = None;
        *self.last_sample.borrow_mut() = None;
        *self.watched_accumulator.borrow_mut() = 0.0;
    }
}
