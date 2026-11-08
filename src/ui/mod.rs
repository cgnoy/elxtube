mod home;
mod channel;
mod list_view;
mod video_item;
mod window;
pub mod player;

use std::time::Instant;
use gtk::prelude::*;
use gtk::CssProvider;

pub fn run(t0: Instant, conn: rusqlite::Connection) -> anyhow::Result<()> {
    let app = gtk::Application::builder()
        .application_id("io.github.yahay.Elxtube")
        .build();

    let conn = std::rc::Rc::new(std::cell::RefCell::new(conn));
    
    app.connect_startup(|_| {
        // Reset semua GTK settings agar tidak mewarisi ricing
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_theme_name(Some("Adwaita"));
            settings.set_gtk_icon_theme_name(Some("Adwaita"));
            settings.set_gtk_font_name(Some("Sans 10"));
            settings.set_gtk_application_prefer_dark_theme(false);
            settings.set_gtk_enable_animations(false);
            settings.set_gtk_decoration_layout(Some(":minimize,maximize,close"));
        }

        // Load CSS dengan prioritas maksimal
        let provider = CssProvider::new();
        provider.load_from_data(include_str!("style.css"));
        
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().unwrap(),
            &provider,
            u32::MAX - 1, // Sangat tinggi, di atas STYLE_PROVIDER_PRIORITY_USER
        );
    });

    app.connect_activate(move |app| {
        window::build(app, t0, conn.clone());
    });

    app.run();
    Ok(())
}
