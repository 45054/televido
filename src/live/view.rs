// SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org>
// SPDX-FileCopyrightText: 2026 Televido contributors
// SPDX-FileContributor: Claude (Anthropic)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::{Cell, OnceCell};

use adw::{gio, glib, gtk, prelude::*, subclass::prelude::*};
use futures_util::StreamExt;
use gettextrs::gettext;
use smart_default::SmartDefault;

use crate::{
    application::TvApplication,
    settings::TvSettings,
    utils::{spawn, tokio},
    zapp::{ChannelId, ChannelInfo, Show, ShowsResult},
};

use super::{card::TvLiveCard, channels::ChannelObject, filter::channel_filter};

/// The number of programme requests that run at the same time.
const MAX_PARALLEL_REQUESTS: usize = 6;

mod imp {
    use super::*;

    #[derive(Debug, SmartDefault, gtk::CompositeTemplate)]
    #[template(file = "src/live/view.blp")]
    pub struct TvLiveView {
        #[template_child]
        stack: TemplateChild<gtk::Stack>,
        #[template_child]
        search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        results_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        channels_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        error_page: TemplateChild<adw::StatusPage>,

        filter: OnceCell<gtk::CustomFilter>,
        /// Incremented on every reload to discard outdated results.
        generation: Cell<u64>,
    }

    impl TvLiveView {
        async fn load_channels(&self) -> eyre::Result<gio::ListStore> {
            let client = TvApplication::get().zapp();
            let settings = TvSettings::get();
            let visible_channels = settings.live_channels();

            let live_channels = TvApplication::get()
                .live_channels()
                .await
                .map_err(|e| eyre::eyre!("{e:#}"))?;

            let live_channels: Vec<(ChannelId, ChannelInfo)> = if visible_channels.is_empty() {
                live_channels
                    .iter()
                    .map(|(channel_id, channel_info)| (channel_id.clone(), channel_info.clone()))
                    .collect()
            } else {
                visible_channels
                    .into_iter()
                    .filter_map(|channel_id| {
                        let channel_info = live_channels.get(&channel_id)?;
                        Some((channel_id, channel_info.clone()))
                    })
                    .collect()
            };

            if live_channels.is_empty() {
                eyre::bail!(gettext("No livestream channels available"));
            }

            // the programme of a channel is optional, a failing request only
            // leaves its card without one
            let channels = tokio(async move {
                let channel_ids: Vec<ChannelId> =
                    live_channels.iter().map(|(id, _)| id.clone()).collect();
                let shows: Vec<Option<Vec<Show>>> = futures_util::stream::iter(channel_ids)
                    .map(|channel_id| {
                        let client = client.clone();
                        async move {
                            match client.shows(&channel_id).await {
                                Ok(ShowsResult::Shows(shows)) => Some(shows),
                                Ok(ShowsResult::Error(e)) => {
                                    tracing::debug!("no shows for channel “{channel_id}”: {e}");
                                    None
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "failed to load the shows of channel “{channel_id}”: {e:?}"
                                    );
                                    None
                                }
                            }
                        }
                    })
                    .buffered(MAX_PARALLEL_REQUESTS)
                    .collect()
                    .await;

                live_channels.into_iter().zip(shows).collect::<Vec<_>>()
            })
            .await;

            let channel_objects = channels
                .into_iter()
                .map(|((channel_id, channel_info), shows)| {
                    let channel = ChannelObject::new(
                        channel_id.as_ref(),
                        &channel_info.name,
                        &channel_info.stream_url,
                    );
                    if let Some(Show {
                        title,
                        subtitle,
                        description,
                        channel: _,
                        start_time,
                        end_time,
                    }) = shows.as_ref().and_then(|shows| shows.first())
                    {
                        channel.set_title(Some(title.as_str()));
                        channel.set_subtitle(subtitle.as_deref());
                        channel.set_description(
                            description.as_deref().map(html2pango::markup).as_deref(),
                        );
                        channel.set_start_time(start_time.unix_timestamp());
                        channel.set_end_time(end_time.unix_timestamp());
                    }
                    channel
                })
                .collect::<gio::ListStore>();

            Ok(channel_objects)
        }
        pub(super) async fn reload(&self) {
            let generation = self.generation.get() + 1;
            self.generation.set(generation);
            self.stack.set_visible_child_name("spinner");

            let result = self.load_channels().await;
            // a newer reload has started in the meantime
            if self.generation.get() != generation {
                return;
            }

            match result {
                Ok(channels) => {
                    let channels =
                        gtk::FilterListModel::new(Some(channels), self.filter.get().cloned());
                    channels.connect_items_changed(glib::clone!(
                        #[weak(rename_to = slf)]
                        self,
                        move |channels, _, _, _| slf.show_results(channels.n_items())
                    ));
                    self.show_results(channels.n_items());

                    self.channels_list.bind_model(Some(&channels), |channel| {
                        glib::Object::builder::<TvLiveCard>()
                            .property("channel", channel)
                            .build()
                            .upcast()
                    });
                    self.stack.set_visible_child_name("channels");
                }
                Err(e) => {
                    tracing::error!("{e:?}");
                    self.error_page
                        .set_description(Some(&glib::markup_escape_text(&format!("{e:#}"))));
                    self.stack.set_visible_child_name("error");
                }
            }
        }

        /// Shows the list, or a hint if no channel matches the filter.
        fn show_results(&self, matches: u32) {
            let page = if matches == 0 && !self.search_entry.text().is_empty() {
                "no-matches"
            } else {
                "list"
            };
            self.results_stack.set_visible_child_name(page);
        }

        pub(super) fn focus_search(&self) {
            self.search_entry.grab_focus();
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TvLiveView {
        const NAME: &'static str = "TvLiveView";
        type Type = super::TvLiveView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TvLiveView {
        fn constructed(&self) {
            self.parent_constructed();

            let settings = TvSettings::get();

            self.filter
                .set(channel_filter(&self.search_entry))
                .expect("the view is constructed once");
            // typing anywhere in the view filters the channels
            self.search_entry.set_key_capture_widget(Some(&*self.obj()));

            self.channels_list.connect_row_activated(|_, row| {
                let row = row
                    .downcast_ref::<TvLiveCard>()
                    .expect("invalid ListBoxRow type");
                row.set_expanded(!row.expanded())
            });

            let slf = self.to_owned();
            spawn(async move { slf.reload().await });

            settings.connect_live_channels_changed(glib::clone!(
                #[weak(rename_to = slf)]
                self,
                move |_| {
                    spawn(async move { slf.reload().await });
                }
            ));
        }
    }
    impl WidgetImpl for TvLiveView {}
    impl BinImpl for TvLiveView {}
}

glib::wrapper! {
    pub struct TvLiveView(ObjectSubclass<imp::TvLiveView>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TvLiveView {
    /// Reloads the channel list and the programme, e.g. after an error.
    pub fn reload(&self) {
        TvApplication::get().live_channels().load();
        let slf = self.imp().to_owned();
        spawn(async move { slf.reload().await });
    }

    pub fn focus_search(&self) {
        self.imp().focus_search();
    }
}
