use anyhow::Result;

use crate::core::database::Database;

#[derive(Clone)]
pub struct SettingsService {
    database: Database,
}

impl SettingsService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        self.database
            .with_read_conn(|conn| crate::core::db::settings_get(conn, key))
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.database
            .with_conn(|conn| crate::core::db::settings_set(conn, key, value))
    }
}
