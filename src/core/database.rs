use anyhow::{anyhow, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use super::{db, paths};

#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
    readers: Arc<Vec<Mutex<Connection>>>,
    next_reader: Arc<AtomicUsize>,
}

impl Database {
    pub fn open() -> Result<Self> {
        Self::open_at(&paths::db_path()?)
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        let connection = db::open_at(path)?;
        let mut readers = Vec::with_capacity(2);
        for _ in 0..2 {
            let reader = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            db::configure_connection(&reader)?;
            readers.push(Mutex::new(reader));
        }
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            readers: Arc::new(readers),
            next_reader: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        Ok(Self::from_connection(db::open_in_memory()?))
    }

    pub fn from_connection(connection: Connection) -> Self {
        Self {
            connection: Arc::new(Mutex::new(connection)),
            readers: Arc::new(Vec::new()),
            next_reader: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn borrow(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn with_conn<T>(&self, operation: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow!("database lock is poisoned"))?;
        operation(&connection)
    }

    pub fn with_read_conn<T>(&self, operation: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        if self.readers.is_empty() {
            return self.with_conn(operation);
        }
        let index = self.next_reader.fetch_add(1, Ordering::Relaxed) % self.readers.len();
        let connection = self.readers[index]
            .lock()
            .map_err(|_| anyhow!("database reader lock is poisoned"))?;
        operation(&connection)
    }
}
