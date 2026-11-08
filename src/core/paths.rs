use anyhow::Result;
use std::path::PathBuf;

fn xdg_dir(var: &str, fallback: &[&str]) -> PathBuf {
    if let Some(v) = std::env::var_os(var) {
        return PathBuf::from(v);
    }
    let home = std::env::var_os("HOME").unwrap_or_default();
    let mut p = PathBuf::from(home);
    for seg in fallback {
        p.push(seg);
    }
    p
}

pub fn data_dir() -> Result<PathBuf> {
    let dir = xdg_dir("XDG_DATA_HOME", &[".local", "share"]).join("elxtube");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn cache_dir() -> Result<PathBuf> {
    let dir = xdg_dir("XDG_CACHE_HOME", &[".cache"]).join("elxtube");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("state.db"))
}

pub fn thumbs_dir() -> Result<PathBuf> {
    let d = cache_dir()?.join("thumbs");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}
