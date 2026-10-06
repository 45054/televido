// SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::{gdk, glib, gtk, prelude::*, subclass::prelude::*};
use gettextrs::{gettext, ngettext};

use crate::{
    application::TvApplication,
    mpv,
    settings::TvSettings,
    utils::{show_error, spawn, tokio},
};

use super::{ChannelList, FritzChannel, FritzChannels, LogoIndex};

const LOGO_SIZE: i32 = 48;

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
    }

    impl TvFritzView {
        pub(super) async fn reload(&self) {
            let settings = TvSettings::get();
            if !settings.fritztv_enabled() {
                return;
            }

            let generation = self.generation.get() + 1;
            self.generation.set(generation);
            self.stack.set_visible_child_name("spinner");

            let fritzbox = TvApplication::get().fritzbox();
            let address = settings.fritzbox_address();
            let lists = ChannelList::selected(settings.fritztv_show_radio());
            self.lists.replace(lists.clone());
            let result = tokio(async move { fritzbox.channels(&address, &lists).await }).await;

            if self.generation.get() != generation {
                return;
            }

            match result {
                Ok(channels) if channels.is_empty() => {
                    self.stack.set_visible_child_name("empty");
                }
                Ok(channels) => {
                    let logos = self.show_channels(&channels);
                    self.stack.set_visible_child_name("channels");
                    self.load_logos(logos).await;
                }
                Err(e) => {
                    tracing::error!("{e:?}");
                    self.error_page
                        .set_description(Some(&glib::markup_escape_text(&format!("{e:#}"))));
                    self.stack.set_visible_child_name("error");
                }
            }
        }

        /// Fills the channel lists and returns the logo placeholders by channel name.
        fn show_channels(&self, channels: &FritzChannels) -> Vec<(String, gtk::Image)> {
            let mut logos = Vec::new();

            for (list, group, list_box) in [
                (ChannelList::Hd, &self.hd_group, &self.hd_list),
                (ChannelList::Sd, &self.sd_group, &self.sd_list),
                (ChannelList::Radio, &self.radio_group, &self.radio_list),
            ] {
                let channels = channels.get(list);

                list_box.remove_all();
                group.set_visible(!channels.is_empty());
                group.set_description(Some(
                    &ngettext("{} channel", "{} channels", channels.len() as u32)
                        .replace("{}", &channels.len().to_string()),
                ));

                for channel in channels {
                    let (row, logo) = channel_row(channel, list);
                    list_box.append(&row);
                    logos.push((channel.name.clone(), logo));
                }
            }

            logos
        }

        async fn load_logos(&self, logos: Vec<(String, gtk::Image)>) {
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

            for (name, image) in logos {
                let Some(file_name) = index.lookup(&name).map(ToOwned::to_owned) else {
                    continue;
                };
                let fritzbox = fritzbox.clone();

                spawn(async move {
                    let data = match tokio(async move { fritzbox.logo(file_name).await }).await {
                        Ok(data) => data,
                        Err(e) => {
                            tracing::debug!("failed to load logo for “{name}”: {e:?}");
                            return;
                        }
                    };
                    match gdk::Texture::from_bytes(&glib::Bytes::from_owned(data)) {
                        Ok(texture) => {
                            image.remove_css_class("dim-label");
                            image.set_pixel_size(LOGO_SIZE);
                            image.set_paintable(Some(&texture));
                        }
                        Err(e) => tracing::debug!("invalid logo for “{name}”: {e:?}"),
                    }
                });
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
            settings.connect_fritztv_show_radio_changed(glib::clone!(
                #[weak(rename_to = slf)]
                self,
                move |settings| {
                    // the preferences dialog rewrites the key when it is opened
                    if ChannelList::selected(settings.fritztv_show_radio()) != *slf.lists.borrow() {
                        spawn(async move { slf.reload().await })
                    }
                }
            ));
        }
    }
    impl WidgetImpl for TvFritzView {}
    impl BinImpl for TvFritzView {}
}

glib::wrapper! {
    pub struct TvFritzView(ObjectSubclass<imp::TvFritzView>)
        @extends gtk::Widget, adw::Bin;
}

impl TvFritzView {
    pub fn reload(&self) {
        let slf = self.imp().to_owned();
        spawn(async move { slf.reload().await });
    }
}

fn channel_row(channel: &FritzChannel, list: ChannelList) -> (adw::ActionRow, gtk::Image) {
    let row = adw::ActionRow::builder()
        .title(&channel.name)
        .use_markup(false)
        .activatable(true)
        .build();

    let logo = gtk::Image::builder()
        .icon_name(match list {
            ChannelList::Radio => "audio-x-generic-symbolic",
            ChannelList::Hd | ChannelList::Sd => "tv-symbolic",
        })
        .pixel_size(24)
        .width_request(LOGO_SIZE)
        .height_request(LOGO_SIZE)
        .css_classes(["dim-label"])
        .build();
    row.add_prefix(&logo);

    let play_button = gtk::Button::builder()
        .icon_name("play-symbolic")
        .tooltip_text(gettext("Play"))
        .valign(gtk::Align::Center)
        .css_classes(["flat", "circular"])
        .build();
    row.add_suffix(&play_button);

    let play = {
        let name = channel.name.clone();
        let url = channel.url.clone();
        move || {
            let name = name.clone();
            let url = url.clone();
            spawn(async move {
                if let Err(e) = mpv::play(&name, &url).await {
                    show_error(e);
                }
            });
        }
    };
    row.connect_activated({
        let play = play.clone();
        move |_| play()
    });
    play_button.connect_clicked(move |_| play());

    (row, logo)
}
