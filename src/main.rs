use std::time::Instant;

fn rss_kb() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest.trim().trim_end_matches("kB").trim().parse().unwrap_or(0);
        }
    }
    0
}

fn main() -> anyhow::Result<()> {
    let t0 = Instant::now();
    eprintln!("[startup] begin (RSS {} KB)", rss_kb());

    std::env::set_var("GSK_RENDERER", "gl");
    std::env::set_var("GDK_BACKEND", "x11");

    let conn = elxtube::core::db::open()?;
    eprintln!("[startup] db ready {}ms", t0.elapsed().as_millis());

    // Tidak ada lagi seed_demo_data!
    
    elxtube::ui::run(t0, conn)
}
