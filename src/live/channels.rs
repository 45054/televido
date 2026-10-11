// SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org>
// SPDX-FileCopyrightText: 2026 Televido contributors
// SPDX-FileContributor: Claude (Anthropic)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::{Cell, RefCell};

use adw::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::utils::format_timestamp_time;

/// Where a channel comes from and how it is played.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChannelKind {
    /// A livestream from the Zapp API, played with mpv or an external player.
    #[default]
    Zapp,
    /// A DVB-C channel of a FRITZ!Box, played with mpv.
    Fritz,
    /// A DVB-C radio channel of a FRITZ!Box, played with mpv.
    FritzRadio,
}

mod imp {
    use super::*;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::ChannelObject)]
    pub struct ChannelObject {
        #[property(get, construct_only)]
        id: RefCell<String>,
        #[property(get, construct_only)]
        name: RefCell<String>,
        #[property(get, set, nullable)]
        title: RefCell<Option<String>>,
        #[property(get, set, nullable)]
        subtitle: RefCell<Option<String>>,
        #[property(get, set, nullable)]
        description: RefCell<Option<String>>,
        #[property(get, set)]
        start_time: Cell<i64>,
        #[property(get, set)]
        end_time: Cell<i64>,
        #[property(get)]
        duration: Cell<i64>,
        #[property(get)]
        timespan: RefCell<Option<String>>,
        #[property(get, construct_only)]
        stream_url: RefCell<String>,
        /// The channel logo, if it isn't one of the bundled icons.
        #[property(get, set, nullable)]
        logo: RefCell<Option<gdk::Paintable>>,
        /// The event after the current one, e.g. “Next: 21:45 Tagesthemen”.
        #[property(get, set, nullable)]
        next: RefCell<Option<String>>,

        pub(super) kind: Cell<ChannelKind>,
    }

    impl ChannelObject {
        fn update_time(&self) {
            self.update_timespan();
            self.update_duration();
        }
        fn update_timespan(&self) {
            let obj = self.obj();
            let start_time = obj.start_time();
            let end_time = obj.end_time();

            let timespan = if start_time == 0 || end_time == 0 {
                None
            } else if let (Some(start_time), Some(end_time)) = (
                format_timestamp_time(start_time),
                format_timestamp_time(end_time),
            ) {
                Some(format!("{start_time} - {end_time}",))
            } else {
                None
            };

            self.timespan.replace(timespan);
            obj.notify_timespan();
        }
        fn update_duration(&self) {
            let obj = self.obj();
            let start_time = obj.start_time();
            let end_time = obj.end_time();

            let duration = if start_time == 0 || end_time == 0 {
                0
            } else {
                end_time - start_time
            };

            self.duration.replace(duration);
            obj.notify_duration();
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChannelObject {
        const NAME: &'static str = "ChannelObject";
        type Type = super::ChannelObject;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for ChannelObject {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            obj.connect_start_time_notify(|obj| obj.imp().update_time());
            obj.connect_end_time_notify(|obj| obj.imp().update_time());
        }
    }
}

glib::wrapper! {
    pub struct ChannelObject(ObjectSubclass<imp::ChannelObject>);
}

impl ChannelObject {
    pub fn new(id: &str, name: &str, stream_url: &str) -> Self {
        glib::Object::builder()
            .property("id", id)
            .property("name", name)
            .property("stream-url", stream_url)
            .build()
    }

    /// Creates a channel of a FRITZ!Box. Its `stream_url` is played with mpv.
    pub fn new_fritz(name: &str, stream_url: &str, kind: ChannelKind) -> Self {
        let slf = Self::new("", name, stream_url);
        slf.imp().kind.set(kind);
        slf
    }

    pub fn kind(&self) -> ChannelKind {
        self.imp().kind.get()
    }
}
