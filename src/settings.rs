// SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org>
// SPDX-FileCopyrightText: 2026 Televido contributors
// SPDX-FileContributor: Claude (Anthropic)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::OnceCell;

use adw::{gio, glib, prelude::*};
use gsettings_macro::gen_settings;

use crate::{config::BASE_APP_ID, zapp::ChannelId};

#[gen_settings(file = "data/io.github._45054.TelevidoFritz.gschema.xml")]
#[gen_settings_define(
    key_name = "live-channels",
    arg_type = "Vec<ChannelId>",
    ret_type = "Vec<ChannelId>"
)]
pub struct TvSettings;

impl TvSettings {
    pub fn get() -> Self {
        thread_local! {
            static SETTINGS: OnceCell<TvSettings> = const { OnceCell::new() };
        }
        SETTINGS.with(|settings| {
            settings
                .get_or_init(|| TvSettings::new(BASE_APP_ID))
                .clone()
        })
    }
}

/// The settings of the previous app ID `de.k_bo.Televido`, read with the
/// relocatable schema `<APP_ID>.Legacy` at their previous path.
const LEGACY_PATH: &str = "/de/k_bo/televido/";

impl TvSettings {
    /// Takes over the settings that were changed under the previous app ID,
    /// once.
    pub fn migrate_legacy(&self) {
        if self.legacy_settings_migrated() {
            return;
        }
        self.set_legacy_settings_migrated(true);

        let legacy_schema_id = format!("{BASE_APP_ID}.Legacy");
        let Some(legacy_schema) = gio::SettingsSchemaSource::default()
            .and_then(|source| source.lookup(&legacy_schema_id, true))
        else {
            tracing::warn!("schema {legacy_schema_id} is not installed");
            return;
        };
        let Some(schema) = self.settings_schema() else {
            return;
        };
        let legacy = gio::Settings::with_path(&legacy_schema_id, LEGACY_PATH);

        for key in legacy_schema.list_keys() {
            // only values the user has changed
            let Some(value) = legacy.user_value(&key) else {
                continue;
            };
            // e.g. a value that the current choices no longer allow
            if !schema.has_key(&key) || !schema.key(&key).range_check(&value) {
                tracing::warn!("not taking over the previous setting {key} = {value}");
                continue;
            }
            match self.set_value(&key, &value) {
                Ok(()) => tracing::info!("took over the previous setting {key} = {value}"),
                Err(e) => tracing::warn!("failed to take over the previous setting {key}: {e}"),
            }
        }
    }
}

impl Default for TvSettings {
    fn default() -> Self {
        Self::get()
    }
}

impl StaticVariantType for ChannelId {
    fn static_variant_type() -> std::borrow::Cow<'static, glib::VariantTy> {
        String::static_variant_type()
    }
}
impl FromVariant for ChannelId {
    fn from_variant(variant: &glib::Variant) -> Option<Self> {
        String::from_variant(variant).map(ChannelId::from)
    }
}
impl ToVariant for ChannelId {
    fn to_variant(&self) -> glib::Variant {
        AsRef::<str>::as_ref(self).to_variant()
    }
}
impl From<ChannelId> for glib::Variant {
    fn from(id: ChannelId) -> Self {
        id.to_variant()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VideoQuality {
    High,
    Medium,
    Low,
}
impl VideoQuality {
    pub fn default_playback() -> Self {
        // the schema only allows these values
        match TvSettings::get().default_playback_quality() {
            DefaultPlaybackQuality::High => VideoQuality::High,
            DefaultPlaybackQuality::Medium => VideoQuality::Medium,
            DefaultPlaybackQuality::Low => VideoQuality::Low,
        }
    }
}
