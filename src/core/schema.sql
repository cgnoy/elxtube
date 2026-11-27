PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA cache_size = -8000;
PRAGMA mmap_size = 0;
PRAGMA temp_store = MEMORY;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS feed_cache (
  video_id TEXT PRIMARY KEY,
  channel_id TEXT,
  channel_name TEXT,
  title TEXT NOT NULL,
  duration INTEGER,
  thumbnail_url TEXT,
  published_at INTEGER,
  source TEXT NOT NULL DEFAULT 'cache',
  fetched_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_feed_cache_channel ON feed_cache(channel_id);
CREATE INDEX IF NOT EXISTS idx_feed_cache_fetched ON feed_cache(fetched_at);
CREATE INDEX IF NOT EXISTS idx_feed_cache_published ON feed_cache(published_at DESC);

CREATE TABLE IF NOT EXISTS history (
  video_id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  channel_id TEXT,
  channel_name TEXT,
  duration INTEGER,
  last_position REAL DEFAULT 0,
  last_watched INTEGER NOT NULL,
  thumbnail_url TEXT
);
CREATE INDEX IF NOT EXISTS idx_history_watched ON history(last_watched DESC);