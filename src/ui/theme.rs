use gtk::CssProvider;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("light") => Self::Light,
            _ => Self::Dark,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }
}

#[derive(Clone)]
pub struct ThemeController {
    current: Rc<Cell<Theme>>,
    provider: Rc<RefCell<Option<CssProvider>>>,
}

impl ThemeController {
    pub fn new(initial: Theme) -> Self {
        Self {
            current: Rc::new(Cell::new(initial)),
            provider: Rc::new(RefCell::new(None)),
        }
    }

    pub fn current(&self) -> Theme {
        self.current.get()
    }

    pub fn install(&self) {
        let display = gtk::gdk::Display::default().expect("GTK display");
        let base = CssProvider::new();
        base.load_from_data(include_str!("style.css"));
        let provider = CssProvider::new();
        gtk::style_context_add_provider_for_display(&display, &base, u32::MAX - 2);
        gtk::style_context_add_provider_for_display(&display, &provider, u32::MAX - 1);
        *self.provider.borrow_mut() = Some(provider);
        self.apply(self.current());
    }

    pub fn apply(&self, theme: Theme) {
        self.current.set(theme);
        if let Some(provider) = self.provider.borrow().as_ref() {
            provider.load_from_data(match theme {
                Theme::Dark => include_str!("theme_dark.css"),
                Theme::Light => include_str!("theme_light.css"),
            });
        }
    }
}
