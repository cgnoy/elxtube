mod channel;
mod home;
mod i18n;
mod layout;
mod list_view;
mod navigation;
mod player;
mod playlist;
mod status;
mod theme;
mod thumbnails;
mod video_item;
mod window;

use crate::application::AppContext;
use gtk::prelude::*;
use std::time::Instant;
use theme::{Theme, ThemeController};

pub fn run(t0: Instant, app_context: AppContext) -> anyhow::Result<()> {
    let app = gtk::Application::builder()
        .application_id("io.github.yahay.Elxtube")
        .build();

    let conn = app_context;
    let saved_theme = conn.settings.get("theme").ok().flatten();
    let theme = ThemeController::new(Theme::parse(saved_theme.as_deref()));

    let startup_theme = theme.clone();
    app.connect_startup(move |_| {
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_theme_name(Some("Adwaita"));
            settings.set_gtk_icon_theme_name(Some("Adwaita"));
            settings.set_gtk_font_name(Some("Sans 10"));
            settings.set_gtk_enable_animations(false);
            settings.set_gtk_decoration_layout(Some(":minimize,maximize,close"));
        }
        startup_theme.install();
    });

    app.connect_activate(move |app| {
        window::build(app, t0, conn.clone(), theme.clone());
    });

    app.run();
    Ok(())
}
