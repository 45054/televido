// SPDX-FileCopyrightText: 2026 Televido contributors
// SPDX-FileContributor: Claude (Anthropic)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
};

use adw::{gdk, gio, glib, gtk, prelude::*, subclass::prelude::*};
use gettextrs::{gettext, ngettext};

use crate::{
    application::TvApplication,
    live::{channel_filter, ChannelKind, ChannelObject, TvLiveCard},
    settings::TvSettings,
    utils::{format_timestamp_time, spawn, tokio},
};

use super::{unix_now, ChannelList, FritzChannel, FritzChannels, LogoIndex, LogoSources, NowNext};

/// How often the programme information of the cards is updated, so that
/// the next show appears when the current one has ended.
const EPG_REFRESH_SECONDS: u32 = 30;

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(file = "src/fritzbox/view.blp")]
    pub struct TvFritzView {
        #[template_child]
        stack: TemplateChild<gtk::Stack>,
        #[template_child]
        error_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        results_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        tv_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        tv_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        hd_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        hd_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        sd_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        sd_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        radio_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        radio_list: TemplateChild<gtk::ListBox>,

        logo_index: RefCell<Option<Rc<LogoIndex>>>,
        /// The channel lists of the last reload.
        lists: RefCell<Vec<ChannelList>>,
        /// Incremented on every reload to discard outdated results.
        generation: Cell<u64>,

        filter: OnceCell<gtk::CustomFilter>,
        /// The groups and their channels that match the filter.
        filtered: RefCell<Vec<(adw::PreferencesGroup, gtk::FilterListModel)>>,

        /// The cards' channels of the last reload.
        channel_objects: RefCell<Vec<ChannelObject>>,
        /// The TV channels of the last reload, which have programme
        /// information.
        epg_channels: RefCell<Vec<FritzChannel>>,
        /// Incremented whenever the programme guide is (re)started or stopped.
        epg_generation: Cell<u64>,
    }

    impl TvFritzView {
        pub(super) async fn reload(&self) {
            let settings = TvSettings::get();
            if !settings.fritztv_enabled() {
                self.epg_channels.take();
                self.update_epg();
                return;
            }

            let generation = self.generation.get() + 1;
            self.generation.set(generation);
            self.stack.set_visible_child_name("spinner");

            let fritzbox = TvApplication::get().fritzbox();
            let address = settings.fritzbox_address();
            let lists = selected_lists();
            self.lists.replace(lists.clone());
            let result = tokio(async move { fritzbox.channels(&address, &lists).await }).await;

            if self.generation.get() != generation {
                return;
            }

            match result {
                Ok(channels) if channels.is_empty() => {
                    self.channel_objects.take();
                    self.epg_channels.take();
                    self.update_epg();
                    self.stack.set_visible_child_name("empty");
                }
                Ok(channels) => {
                    let logos = self.show_channels(&channels);
                    // radio channels hardly ever have programme information
                    self.epg_channels.replace(
                        [&channels.tv, &channels.hd, &channels.sd]
                            .into_iter()
                            .flatten()
                            .cloned()
                            .collect(),
                    );
                    self.update_epg();
                    self.stack.set_visible_child_name("channels");
                    self.load_logos(logos).await;
                }
                Err(e) => {
                    self.channel_objects.take();
                    self.epg_channels.take();
                    self.update_epg();
                    tracing::error!("{e:?}");
                    self.error_page
                        .set_description(Some(&glib::markup_escape_text(&format!("{e:#}"))));
                    self.stack.set_visible_child_name("error");
                }
            }
        }

        /// Fills the channel lists and returns the channels by list and name
        /// to load their logos.
        fn show_channels(
            &self,
            channels: &FritzChannels,
        ) -> Vec<(ChannelList, String, ChannelObject)> {
            let mut logos = Vec::new();
            let mut objects = Vec::new();
            let mut filtered = Vec::new();

            for (list, group, list_box) in [
                (ChannelList::Tv, &self.tv_group, &self.tv_list),
                (ChannelList::Hd, &self.hd_group, &self.hd_list),
                (ChannelList::Sd, &self.sd_group, &self.sd_list),
                (ChannelList::Radio, &self.radio_group, &self.radio_list),
            ] {
                let channels = channels.get(list);

                let kind = match list {
                    ChannelList::Radio => ChannelKind::FritzRadio,
                    ChannelList::Tv | ChannelList::Hd | ChannelList::Sd => ChannelKind::Fritz,
                };
                let store = gio::ListStore::new::<ChannelObject>();
                for channel in channels {
                    let object = ChannelObject::new_fritz(&channel.name, &channel.url, kind);
                    store.append(&object);
                    logos.push((list, channel.name.clone(), object.clone()));
                    objects.push(object);
                }
                let model = gtk::FilterListModel::new(Some(store), self.filter.get().cloned());
                model.connect_items_changed(glib::clone!(
                    #[weak(rename_to = slf)]
                    self,
                    move |_, _, _, _| slf.update_groups()
                ));
                list_box.bind_model(Some(&model), |channel| {
                    glib::Object::builder::<TvLiveCard>()
                        .property("channel", channel)
                        .build()
                        .upcast()
                });
                filtered.push((group.get(), model));
            }

            self.filtered.replace(filtered);
            self.update_groups();
            self.channel_objects.replace(objects);
            self.refresh_epg();

            logos
        }

        /// Shows the groups with channels that match the filter, and how
        /// many, or a hint if there are none.
        fn update_groups(&self) {
            let mut any = false;
            for (group, model) in self.filtered.borrow().iter() {
                let matches = model.n_items();
                any |= matches > 0;
                group.set_visible(matches > 0);
                group.set_description(Some(
                    &ngettext("{} channel", "{} channels", matches)
                        .replace("{}", &matches.to_string()),
                ));
            }

            let page = if any || self.search_entry.text().is_empty() {
                "list"
            } else {
                "no-matches"
            };
            self.results_stack.set_visible_child_name(page);
        }

        pub(super) fn focus_search(&self) {
            self.search_entry.grab_focus();
        }

        /// (Re)starts reading the programme guide of the channels of the
        /// last reload, or stops it if it is disabled.
        fn update_epg(&self) {
            let generation = self.epg_generation.get() + 1;
            self.epg_generation.set(generation);

            let fritzbox = TvApplication::get().fritzbox();
            fritzbox.epg().stop();

            let channels = self.epg_channels.borrow().clone();
            if channels.is_empty() || !TvSettings::get().fritztv_epg() {
                self.refresh_epg();
                return;
            }

            // show what is known from earlier runs
            self.refresh_epg();

            let view = self.obj().downgrade();
            spawn(async move {
                let Some(current) = view.upgrade() else {
                    return;
                };
                if current.imp().epg_generation.get() != generation {
                    return;
                }
                drop(current);

                let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
                tokio(async move { fritzbox.epg().start(channels, sender) }).await;

                // the sweep ends, and with it the channel, when it is
                // stopped or restarted
                while receiver.recv().await.is_some() {
                    while receiver.try_recv().is_ok() {}
                    let Some(view) = view.upgrade() else { break };
                    view.imp().refresh_epg();
                }
            });
        }

        /// Shows the current programme information on the cards.
        fn refresh_epg(&self) {
            let enabled = TvSettings::get().fritztv_epg();
            let fritzbox = TvApplication::get().fritzbox();
            let now = unix_now();

            for channel in self.channel_objects.borrow().iter() {
                let now_next = enabled
                    .then(|| fritzbox.epg().now_next(&channel.stream_url(), now))
                    .flatten();
                show_now_next(channel, now_next.as_ref());
            }
        }

        async fn load_logos(&self, logos: Vec<(ChannelList, String, ChannelObject)>) {
            let fritzbox = TvApplication::get().fritzbox();

            let cached_index = self.logo_index.borrow().clone();
            let index = match cached_index {
                Some(index) => index,
                None => {
                    let index = Rc::new(
                        tokio({
                            let fritzbox = fritzbox.clone();
                            async move { fritzbox.logo_index().await }
                        })
                        .await,
                    );
                    // retry on the next reload if AVM was not reachable
                    if !index.is_empty() {
                        self.logo_index.replace(Some(index.clone()));
                    }
                    index
                }
            };

            for (list, name, channel) in logos {
                let sources = LogoSources::new(&name, list, &index);
                if sources.is_empty() {
                    continue;
                }
                let fritzbox = fritzbox.clone();

                spawn(async move {
                    let data = match tokio(async move { fritzbox.logo(sources).await }).await {
                        Ok(data) => data,
                        Err(e) => {
                            tracing::debug!("failed to load logo for “{name}”: {e:?}");
                            return;
                        }
                    };
                    match gdk::Texture::from_bytes(&glib::Bytes::from_owned(data)) {
                        Ok(texture) => {
                            channel.set_logo(Some(texture.upcast_ref::<gdk::Paintable>()))
                        }
                        Err(e) => tracing::debug!("invalid logo for “{name}”: {e:?}"),
                    }
                });
            }
        }

        /// Reloads if the selected channel lists differ from the loaded ones.
        ///
        /// The preferences dialog rewrites the settings when it is opened.
        fn reload_if_lists_changed(&self) {
            if selected_lists() != *self.lists.borrow() {
                let slf = self.to_owned();
                spawn(async move { slf.reload().await });
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TvFritzView {
        const NAME: &'static str = "TvFritzView";
        type Type = super::TvFritzView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TvFritzView {
        fn constructed(&self) {
            self.parent_constructed();

            let settings = TvSettings::get();

            self.filter
                .set(channel_filter(&self.search_entry))
                .expect("the view is constructed once");
            // typing anywhere in the view filters the channels
            self.search_entry.set_key_capture_widget(Some(&*self.obj()));

            let slf = self.to_owned();
            spawn(async move { slf.reload().await });

            settings.connect_fritztv_enabled_changed(glib::clone!(
                #[weak(rename_to = slf)]
                self,
                move |_| spawn(async move { slf.reload().await })
            ));
            settings.connect_fritzbox_address_changed(glib::clone!(
                #[weak(rename_to = slf)]
                self,
                move |_| spawn(async move { slf.reload().await })
            ));
            settings.connect_fritztv_separate_hd_sd_changed(glib::clone!(
                #[weak(rename_to = slf)]
                self,
                move |_| slf.reload_if_lists_changed()
            ));
            settings.connect_fritztv_show_radio_changed(glib::clone!(
                #[weak(rename_to = slf)]
                self,
                move |_| slf.reload_if_lists_changed()
            ));
            settings.connect_fritztv_epg_changed(glib::clone!(
                #[weak(rename_to = slf)]
                self,
                move |_| slf.update_epg()
            ));

            // a click on a card shows its details, or plays the channel if
            // it has none
            for list_box in [
                &*self.tv_list,
                &*self.hd_list,
                &*self.sd_list,
                &*self.radio_list,
            ] {
                list_box.connect_row_activated(|_, row| {
                    let card = row
                        .downcast_ref::<TvLiveCard>()
                        .expect("invalid ListBoxRow type");
                    let has_details = card.channel().is_some_and(|channel| {
                        channel.description().is_some() || channel.next().is_some()
                    });
                    if has_details {
                        card.set_expanded(!card.expanded());
                    } else if let Err(e) = card.activate_action("card.play", None) {
                        tracing::error!("failed to play the channel: {e:?}");
                    }
                });
            }

            glib::timeout_add_seconds_local(
                EPG_REFRESH_SECONDS,
                glib::clone!(
                    #[weak(rename_to = slf)]
                    self,
                    #[upgrade_or]
                    glib::ControlFlow::Break,
                    move || {
                        slf.refresh_epg();
                        glib::ControlFlow::Continue
                    }
                ),
            );
        }
    }
    impl WidgetImpl for TvFritzView {}
    impl BinImpl for TvFritzView {}
}

glib::wrapper! {
    pub struct TvFritzView(ObjectSubclass<imp::TvFritzView>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TvFritzView {
    pub fn reload(&self) {
        let slf = self.imp().to_owned();
        spawn(async move { slf.reload().await });
    }

    pub fn focus_search(&self) {
        self.imp().focus_search();
    }
}

fn selected_lists() -> Vec<ChannelList> {
    let settings = TvSettings::get();
    ChannelList::selected(
        settings.fritztv_separate_hd_sd(),
        settings.fritztv_show_radio(),
    )
}

/// Shows the running and the following show on a card.
fn show_now_next(channel: &ChannelObject, now_next: Option<&NowNext>) {
    let current = now_next.and_then(|now_next| now_next.current.as_ref());
    let next = now_next.and_then(|now_next| now_next.next.as_ref());

    let title = current.map(|event| event.title.as_str());
    let subtitle = current.and_then(|event| event.subtitle.as_deref());
    // the description is displayed as markup
    let description = current
        .and_then(|event| event.description.as_deref())
        .map(glib::markup_escape_text);
    let description = description.as_deref();
    let (start, end) = current.map_or((0, 0), |event| (event.start, event.end));
    let next = next.and_then(|event| {
        let time = format_timestamp_time(event.start)?;
        // translators: `{time}` is the start time and `{title}` the title of
        // the following show, e.g. “Next: 20:15 Tagesschau”
        Some(
            gettext("Next: {time} {title}")
                .replace("{time}", &time)
                .replace("{title}", &event.title),
        )
    });

    // setting a property notifies even if its value didn't change
    if channel.title().as_deref() != title {
        channel.set_title(title);
    }
    if channel.subtitle().as_deref() != subtitle {
        channel.set_subtitle(subtitle);
    }
    if channel.description().as_deref() != description {
        channel.set_description(description);
    }
    if channel.start_time() != start {
        channel.set_start_time(start);
    }
    if channel.end_time() != end {
        channel.set_end_time(end);
    }
    if channel.next() != next {
        channel.set_next(next.as_deref());
    }
}
