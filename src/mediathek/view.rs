// SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org>
// SPDX-FileCopyrightText: 2026 Televido contributors
// SPDX-FileContributor: Claude (Anthropic)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::{Cell, OnceCell, RefCell},
    sync::Arc,
};

use adw::{gio, glib, gtk, prelude::*, subclass::prelude::*};
use eyre::WrapErr;
use gettextrs::gettext;
use mediathekviewweb::{
    models::{SortField, SortOrder},
    Mediathek,
};

use crate::{
    config::{APP_ID, PROJECT_URL, VERSION},
    settings::TvSettings,
    utils::{show_error, spawn, spawn_clone, tokio},
};

use super::{card::TvMediathekCard, shows::ShowObject};

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate, glib::Properties)]
    #[template(file = "src/mediathek/view.blp")]
    #[properties(wrapper_type = super::TvMediathekView)]
    pub struct TvMediathekView {
        #[template_child]
        search_toolbar: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        status_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        results_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        stack: TemplateChild<gtk::Stack>,
        #[template_child]
        nothing_found_view: TemplateChild<adw::StatusPage>,
        #[template_child]
        results_view: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        loading_view: TemplateChild<adw::Spinner>,
        #[template_child]
        error_view: TemplateChild<adw::StatusPage>,

        #[property(get, set)]
        compact: Cell<bool>,

        #[property(get, set)]
        query_string: RefCell<String>,
        #[property(get, set)]
        include_future: Cell<bool>,
        #[property(get, set)]
        search_everywhere: Cell<bool>,

        #[property(get, set)]
        sort_by: RefCell<String>,
        #[property(get, set)]
        sort_order: RefCell<String>,

        #[property(get, set)]
        total_results: Cell<u64>,
        #[property(get, set)]
        more_available: Cell<bool>,

        pub(super) client: OnceCell<Arc<Mediathek>>,
        pub(super) shows_model: OnceCell<gio::ListStore>,
        /// Incremented on every new search to discard outdated results.
        pub(super) generation: Cell<u64>,
    }
    impl TvMediathekView {
        pub(super) fn shows_model(&self) -> gio::ListStore {
            self.shows_model
                .get_or_init(gio::ListStore::new::<ShowObject>)
                .clone()
        }
        pub(super) fn show_status_page(&self) {
            self.stack.set_visible_child(&*self.status_page)
        }
        /// Shows the results, or a hint if there are none.
        pub(super) fn show_results(&self) {
            let n_items = self.shows_model().n_items();
            if n_items == 0 {
                self.stack.set_visible_child(&*self.nothing_found_view);
            } else {
                self.stack.set_visible_child(&*self.results_view);
            }
            self.obj()
                .set_more_available(u64::from(n_items) < self.total_results.get());
        }
        /// Shows a spinner, unless earlier results are shown, which stay
        /// visible until the new ones arrive.
        pub(super) fn show_loading(&self) {
            if self.stack.visible_child().as_ref() != Some(self.results_view.upcast_ref()) {
                self.stack.set_visible_child(&*self.loading_view);
            }
        }
        pub(super) fn show_error_page(&self, e: &eyre::Report) {
            tracing::error!("{e:?}");
            self.error_view
                .set_description(Some(&glib::markup_escape_text(&format!("{e:#}"))));
            self.stack.set_visible_child(&*self.error_view);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TvMediathekView {
        const NAME: &'static str = "TvMediathekView";
        type Type = super::TvMediathekView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_property_action("mediathek.sort-by", "sort-by");
            klass.install_property_action("mediathek.sort-order", "sort-order");
            klass.install_action_async("mediathek.load-more", None, |slf, _, _| async move {
                slf.load_more().await
            })
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for TvMediathekView {
        fn constructed(&self) {
            self.parent_constructed();

            let slf = self.obj();
            let settings = TvSettings::get();
            settings
                .bind_search_everywhere(&*slf, "search-everywhere")
                .flags(gio::SettingsBindFlags::DEFAULT)
                .build();
            settings
                .bind_include_future(&*slf, "include-future")
                .flags(gio::SettingsBindFlags::DEFAULT)
                .build();
            settings
                .bind_sort_by(&*slf, "sort-by")
                .flags(gio::SettingsBindFlags::DEFAULT)
                .build();
            settings
                .bind_sort_order(&*slf, "sort-order")
                .flags(gio::SettingsBindFlags::DEFAULT)
                .build();

            slf.connect_compact_notify(|slf| {
                let imp = slf.imp();
                if slf.compact() {
                    imp.search_toolbar
                        .set_orientation(gtk::Orientation::Vertical);
                    imp.status_page.add_css_class("compact");
                } else {
                    imp.search_toolbar
                        .set_orientation(gtk::Orientation::Horizontal);
                    imp.status_page.remove_css_class("compact");
                }
            });

            self.results_list
                .bind_model(Some(&self.shows_model()), |show| {
                    glib::Object::builder::<TvMediathekCard>()
                        .property("show", show)
                        .build()
                        .into()
                });

            self.results_list.connect_row_activated(|_, row| {
                let row = row
                    .downcast_ref::<TvMediathekCard>()
                    .expect("invalid ListBoxRow type");
                row.set_expanded(!row.expanded())
            });

            fn load(slf: &super::TvMediathekView) {
                spawn_clone!(slf => slf.load())
            }

            self.search_entry.connect_search_changed(glib::clone!(
                #[weak]
                slf,
                move |_| spawn(async move { slf.load().await })
            ));
            slf.connect_search_everywhere_notify(load);
            slf.connect_include_future_notify(load);
            slf.connect_sort_by_notify(load);
            slf.connect_sort_order_notify(load);

            slf.connect_map(|slf| {
                slf.imp().search_entry.grab_focus();
            });
        }
    }
    impl WidgetImpl for TvMediathekView {}
    impl BinImpl for TvMediathekView {}
}

glib::wrapper! {
    pub struct TvMediathekView(ObjectSubclass<imp::TvMediathekView>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TvMediathekView {
    pub fn focus_search(&self) {
        self.imp().search_entry.grab_focus();
    }

    fn client(&self) -> Arc<Mediathek> {
        self.imp()
            .client
            .get_or_init(|| {
                Arc::new(
                    Mediathek::new(
                        format!("{APP_ID}/{VERSION} ({PROJECT_URL})")
                            .try_into()
                            .expect("invalid user agent"),
                    )
                    .expect("failed to initialize HTTP client"),
                )
            })
            .clone()
    }
    fn query_props(&self) -> QueryProperties {
        let query_string = self.query_string();

        let search_everywhere = self.search_everywhere();
        let include_future = self.include_future();
        let sort_by = match &*self.sort_by() {
            "channel" => SortField::Channel,
            "date" | "timestamp" => SortField::Timestamp,
            "duration" => SortField::Duration,
            _ => SortField::Timestamp,
        };
        let sort_order = match &*self.sort_order() {
            "asc" | "ascending" => SortOrder::Ascending,
            "desc" | "descending" => SortOrder::Descending,
            _ => SortOrder::Descending,
        };

        QueryProperties {
            query_string,
            search_everywhere,
            include_future,
            sort_by,
            sort_order,
        }
    }
    async fn load(&self) {
        let imp = self.imp();
        let generation = imp.generation.get() + 1;
        imp.generation.set(generation);

        let QueryProperties {
            query_string,
            search_everywhere,
            include_future,
            sort_by,
            sort_order,
        } = self.query_props();

        if query_string.is_empty() {
            imp.show_status_page();
            return;
        }

        imp.show_loading();
        let client = self.client();

        let result = tokio(async move {
            client
                .query_string(&query_string, search_everywhere)
                .include_future(include_future)
                .size(15)
                .sort_by(sort_by)
                .sort_order(sort_order)
                .send()
                .await
                .wrap_err_with(|| gettext("Failed to query the MediathekViewWeb API"))
        })
        .await;

        // a newer search has started in the meantime
        if imp.generation.get() != generation {
            return;
        }

        match result {
            Ok(result) => {
                // must be known before the results change
                self.set_total_results(result.query_info.total_results);

                let shows: Vec<ShowObject> =
                    result.results.into_iter().map(ShowObject::new).collect();
                let shows_model = imp.shows_model();
                shows_model.splice(0, shows_model.n_items(), &shows);
                imp.show_results();
            }
            Err(e) => imp.show_error_page(&e),
        }
    }
    async fn load_more(&self) {
        let imp = self.imp();
        let generation = imp.generation.get();

        let QueryProperties {
            query_string,
            search_everywhere,
            include_future,
            sort_by,
            sort_order,
        } = self.query_props();

        if query_string.is_empty() {
            imp.show_status_page();
            return;
        }

        let mut shows_model = imp.shows_model();
        let offset = shows_model.n_items();

        let client = self.client();

        self.action_set_enabled("mediathek.load-more", false);
        let result = tokio(async move {
            client
                .query_string(&query_string, search_everywhere)
                .include_future(include_future)
                .size(15)
                .offset(offset as usize)
                .sort_by(sort_by)
                .sort_order(sort_order)
                .send()
                .await
                .wrap_err_with(|| gettext("Failed to query the MediathekViewWeb API"))
        })
        .await;
        self.action_set_enabled("mediathek.load-more", true);

        // the results belong to a previous search
        if imp.generation.get() != generation {
            return;
        }

        match result {
            Ok(result) => {
                self.set_total_results(result.query_info.total_results);
                shows_model.extend(result.results.into_iter().map(ShowObject::new));
                imp.show_results();
            }
            Err(e) => show_error(e),
        }
    }
    pub fn reload(&self) {
        let slf = self.clone();
        spawn(async move { slf.load().await });
    }
}

#[derive(Debug)]
struct QueryProperties {
    query_string: String,
    search_everywhere: bool,
    include_future: bool,
    sort_by: SortField,
    sort_order: SortOrder,
}
