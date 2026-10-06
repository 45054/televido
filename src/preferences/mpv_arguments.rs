// SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org>
// SPDX-License-Identifier: GPL-3.0-or-later

use adw::{glib, gtk, prelude::*, subclass::prelude::*};
use gettextrs::gettext;

use crate::settings::TvSettings;

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(file = "src/preferences/mpv_arguments.blp")]
    pub struct TvMpvArgumentsPage {
        #[template_child]
        arguments_list: TemplateChild<gtk::ListBox>,

        settings: TvSettings,
    }

    #[gtk::template_callbacks]
    impl TvMpvArgumentsPage {
        #[template_callback]
        fn add_argument(&self, #[rest] _: &[glib::Value]) {
            self.append_row("").grab_focus();
        }
        #[template_callback]
        fn restore_defaults(&self, #[rest] _: &[glib::Value]) {
            self.settings.reset("mpv-arguments");
            self.load();
        }
    }

    impl TvMpvArgumentsPage {
        fn load(&self) {
            self.arguments_list.remove_all();
            for argument in self.settings.mpv_arguments() {
                self.append_row(&argument);
            }
        }

        fn save(&self) {
            let mut arguments = Vec::new();
            let mut child = self.arguments_list.first_child();
            while let Some(row) = child {
                if let Some(row) = row.downcast_ref::<adw::EntryRow>() {
                    let text = row.text();
                    let argument = text.trim();
                    if !argument.is_empty() {
                        arguments.push(argument.to_owned());
                    }
                }
                child = row.next_sibling();
            }

            let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
            self.settings.set_mpv_arguments(&arguments);
        }

        fn append_row(&self, argument: &str) -> adw::EntryRow {
            let row = adw::EntryRow::builder()
                .text(argument)
                .css_classes(["monospace"])
                .build();

            let remove_button = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text(gettext("Remove Argument"))
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            remove_button.connect_clicked(glib::clone!(
                #[weak(rename_to = slf)]
                self,
                #[weak]
                row,
                move |_| {
                    slf.arguments_list.remove(&row);
                    slf.save();
                }
            ));
            row.add_suffix(&remove_button);

            row.connect_changed(glib::clone!(
                #[weak(rename_to = slf)]
                self,
                move |_| slf.save()
            ));

            self.arguments_list.append(&row);
            row
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TvMpvArgumentsPage {
        const NAME: &'static str = "TvMpvArgumentsPage";
        type Type = super::TvMpvArgumentsPage;
        type ParentType = adw::NavigationPage;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TvMpvArgumentsPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.load();
        }
    }
    impl WidgetImpl for TvMpvArgumentsPage {}
    impl NavigationPageImpl for TvMpvArgumentsPage {}
}

glib::wrapper! {
    pub struct TvMpvArgumentsPage(ObjectSubclass<imp::TvMpvArgumentsPage>)
        @extends gtk::Widget, adw::NavigationPage;
}

impl TvMpvArgumentsPage {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
