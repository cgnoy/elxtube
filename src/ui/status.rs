use super::i18n;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Label, Orientation};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone)]
pub struct StatusView {
    container: GtkBox,
    message: Label,
    retry: Button,
    message_key: Rc<Cell<Option<&'static str>>>,
    message_values: Rc<RefCell<Vec<(&'static str, String)>>>,
}

impl StatusView {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Vertical, 10);
        container.set_halign(gtk::Align::Center);
        container.set_valign(gtk::Align::Center);
        container.set_vexpand(true);
        container.set_hexpand(true);
        container.add_css_class("page-status");

        let message = Label::builder()
            .label(i18n::tr("loading"))
            .wrap(true)
            .build();
        message.add_css_class("page-status-message");
        container.append(&message);

        let retry = Button::with_label(i18n::tr("retry"));
        retry.add_css_class("btn-action");
        retry.set_visible(false);
        container.append(&retry);

        Self {
            container,
            message,
            retry,
            message_key: Rc::new(Cell::new(None)),
            message_values: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn apply_language(&self) {
        self.retry.set_label(i18n::tr("retry"));
        if let Some(key) = self.message_key.get() {
            let values = self.message_values.borrow();
            let values = values
                .iter()
                .map(|(name, value)| (*name, value.as_str()))
                .collect::<Vec<_>>();
            self.message.set_text(&i18n::format(key, &values));
        }
    }

    pub fn widget(&self) -> &GtkBox {
        &self.container
    }

    pub fn set_retry_handler<F: Fn() + 'static>(&self, callback: F) {
        self.retry.connect_clicked(move |_| callback());
    }

    fn set_translated(
        &self,
        key: &'static str,
        values: &[(&'static str, &str)],
        retry_visible: bool,
    ) {
        self.message_key.set(Some(key));
        *self.message_values.borrow_mut() = values
            .iter()
            .map(|(name, value)| (*name, (*value).to_string()))
            .collect();
        self.apply_language();
        self.retry.set_visible(retry_visible);
        self.container.set_visible(true);
    }

    pub fn loading_key(&self, key: &'static str) {
        self.set_translated(key, &[], false);
    }

    pub fn empty_key(&self, key: &'static str) {
        self.set_translated(key, &[], false);
    }

    pub fn empty_key_values(&self, key: &'static str, values: &[(&'static str, &str)]) {
        self.set_translated(key, values, false);
    }

    pub fn error_key(&self, key: &'static str, values: &[(&'static str, &str)]) {
        self.set_translated(key, values, true);
    }

    pub fn hide(&self) {
        self.container.set_visible(false);
    }
}
