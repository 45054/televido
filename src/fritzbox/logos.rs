// SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Channel logos provided by AVM at <https://download.avm.de/tv/logos/>.
//!
//! The directory listing is matched against the channel names from the
//! FRITZ!Box playlists (same approach as
//! <https://github.com/ElectronicResearch/fritzmux>). The listing and the
//! downloaded logos are cached in the user's cache directory.

use std::{collections::HashMap, path::PathBuf, sync::OnceLock};

use eyre::{OptionExt, WrapErr};
use tokio::sync::Semaphore;

pub const AVM_LOGOS_URL: &str = "https://download.avm.de/tv/logos/";

const MAX_PARALLEL_DOWNLOADS: usize = 4;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

#[derive(Debug, Default)]
pub struct LogoIndex {
    /// normalized file name (e.g. `zdfhd`) → file name (e.g. `zdf_hd.png`)
    exact: HashMap<String, String>,
    /// normalized file name without quality suffix (e.g. `zdf`) → file name
    stripped: HashMap<String, String>,
}

impl LogoIndex {
    pub fn from_listing(html: &str) -> Self {
        let mut index = Self::default();

        for file_name in png_links(html) {
            let Some((full, stripped)) = keys(file_name.trim_end_matches(".png")) else {
                continue;
            };
            index
                .stripped
                .entry(stripped)
                .or_insert_with(|| file_name.clone());
            index.exact.insert(full, file_name);
        }

        index
    }

    pub fn is_empty(&self) -> bool {
        self.exact.is_empty()
    }

    /// Returns the file name of the logo matching `channel_name`.
    pub fn lookup(&self, channel_name: &str) -> Option<&str> {
        let (full, stripped) = keys(channel_name)?;
        let candidates = [
            full,
            stripped.clone(),
            format!("{stripped}hd"),
            format!("{stripped}sd"),
        ];

        let file_name = candidates
            .iter()
            .find_map(|key| self.exact.get(key))
            .or_else(|| self.stripped.get(&stripped))?;

        Some(file_name)
    }
}

/// Returns the normalized name with and without trailing quality/region
/// suffixes, e.g. `("daserstehd", "daserste")` for `Das Erste HD`.
fn keys(name: &str) -> Option<(String, String)> {
    let name = name
        .to_lowercase()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('ß', "ss");

    let mut words: Vec<&str> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();

    if words.is_empty() {
        return None;
    }

    let full = words.concat();
    while words.len() > 1 && matches!(words.last(), Some(&("hd" | "sd" | "de"))) {
        words.pop();
    }

    Some((full, words.concat()))
}

/// Extracts the file names of all linked `.png` files from a directory listing.
fn png_links(html: &str) -> Vec<String> {
    html.split("href=")
        .skip(1)
        .filter_map(|rest| {
            let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
            let link = rest[1..].split(quote).next()?;
            let file_name = link.rsplit('/').next()?;
            file_name
                .to_lowercase()
                .ends_with(".png")
                .then(|| file_name.to_owned())
        })
        .collect()
}

fn cache_dir() -> PathBuf {
    adw::glib::user_cache_dir().join("televido/fritzbox-logos")
}

/// Fetches the logo index from AVM, falling back to the cached listing.
pub async fn load_index(http: reqwest::Client) -> LogoIndex {
    let cache_file = cache_dir().join("index.html");

    let html = match fetch_listing(&http).await {
        Ok(html) => {
            if let Err(e) = write_cache(&cache_file, html.as_bytes()).await {
                tracing::warn!("failed to cache logo index: {e:?}");
            }
            Some(html)
        }
        Err(e) => {
            tracing::warn!("failed to load logo index from AVM: {e:?}");
            tokio::fs::read_to_string(&cache_file).await.ok()
        }
    };

    html.map(|html| LogoIndex::from_listing(&html))
        .unwrap_or_default()
}

async fn fetch_listing(http: &reqwest::Client) -> eyre::Result<String> {
    Ok(http
        .get(AVM_LOGOS_URL)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?)
}

/// Returns the PNG data of the logo `file_name`, downloading it if necessary.
pub async fn load_logo(http: reqwest::Client, file_name: String) -> eyre::Result<Vec<u8>> {
    static DOWNLOADS: OnceLock<Semaphore> = OnceLock::new();

    let cache_file = cache_dir().join(&file_name);
    if let Ok(data) = tokio::fs::read(&cache_file).await {
        return Ok(data);
    }

    let _permit = DOWNLOADS
        .get_or_init(|| Semaphore::new(MAX_PARALLEL_DOWNLOADS))
        .acquire()
        .await?;

    let url = reqwest::Url::parse(AVM_LOGOS_URL)?.join(&file_name)?;
    let data = http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    data.starts_with(PNG_SIGNATURE)
        .then_some(())
        .ok_or_eyre("logo is not a PNG image")?;

    write_cache(&cache_file, &data)
        .await
        .wrap_err("failed to cache logo")?;

    Ok(data.into())
}

async fn write_cache(path: &std::path::Path, data: &[u8]) -> eyre::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, data).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = r#"<html><body><h1>Index of /tv/logos/</h1><pre>
        <a href="../">../</a>
        <a href="3sat.png">3sat.png</a>
        <a href="arte_hd.png">arte_hd.png</a>
        <a href="das_erste.png">das_erste.png</a>
        <a href="das_erste_hd.png">das_erste_hd.png</a>
        <a href="/tv/logos/zdf.png">zdf.png</a>
        <a href="br_fernsehen_sued_hd.png">br_fernsehen_sued_hd.png</a>
        <a href='prosieben.png'>prosieben.png</a>
        <a href="readme.txt">readme.txt</a>
        </pre></body></html>"#;

    #[test]
    fn parses_listing() {
        let mut links = png_links(LISTING);
        links.sort();
        assert_eq!(
            links,
            [
                "3sat.png",
                "arte_hd.png",
                "br_fernsehen_sued_hd.png",
                "das_erste.png",
                "das_erste_hd.png",
                "prosieben.png",
                "zdf.png",
            ]
        );
    }

    #[test]
    fn matches_channel_names() {
        let index = LogoIndex::from_listing(LISTING);

        assert_eq!(index.lookup("Das Erste HD"), Some("das_erste_hd.png"));
        assert_eq!(index.lookup("Das Erste"), Some("das_erste.png"));
        assert_eq!(index.lookup("ZDF HD"), Some("zdf.png"));
        assert_eq!(index.lookup("arte"), Some("arte_hd.png"));
        assert_eq!(index.lookup("3sat"), Some("3sat.png"));
        assert_eq!(
            index.lookup("BR Fernsehen Süd HD"),
            Some("br_fernsehen_sued_hd.png")
        );
        assert_eq!(index.lookup("ProSieben"), Some("prosieben.png"));
        assert_eq!(index.lookup("Unknown Channel"), None);
        assert_eq!(index.lookup("!!!"), None);
    }

    #[test]
    fn strips_suffixes_but_keeps_single_words() {
        assert_eq!(
            keys("Das Erste HD"),
            Some(("daserstehd".to_owned(), "daserste".to_owned()))
        );
        assert_eq!(keys("HD"), Some(("hd".to_owned(), "hd".to_owned())));
        assert_eq!(
            keys("Sat.1 Gold"),
            Some(("sat1gold".to_owned(), "sat1gold".to_owned()))
        );
    }
}
