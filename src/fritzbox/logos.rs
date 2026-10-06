// SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Channel logos provided by AVM.
//!
//! Logos are first looked up at <https://tv.avm.de/tvapp/logos/>, which is
//! used by the FRITZ!Box web interface (see [`tvapp`]). Otherwise, the
//! directory listing at <https://download.avm.de/tv/logos/> is matched
//! against the channel names from the FRITZ!Box playlists (same approach as
//! <https://github.com/ElectronicResearch/fritzmux>). The listing and the
//! downloaded logos are cached in the user's cache directory.

use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use reqwest::{StatusCode, Url};
use tokio::sync::Semaphore;

use super::{
    tvapp::{self, TVAPP_LOGOS_URL},
    ChannelList,
};

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

/// Where to look for the logo of a channel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogoSources {
    /// Paths relative to [`TVAPP_LOGOS_URL`], most likely first.
    tvapp_paths: Vec<String>,
    /// File name from the directory listing at [`AVM_LOGOS_URL`].
    avm_file_name: Option<String>,
}

impl LogoSources {
    pub fn new(channel_name: &str, list: ChannelList, index: &LogoIndex) -> Self {
        Self {
            tvapp_paths: tvapp::logo_paths(channel_name, list),
            avm_file_name: index.lookup(channel_name).map(ToOwned::to_owned),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tvapp_paths.is_empty() && self.avm_file_name.is_none()
    }

    /// Returns the candidates grouped by source, preferred source first.
    fn files(&self) -> eyre::Result<[Vec<LogoFile>; 2]> {
        let tvapp = self
            .tvapp_paths
            .iter()
            .map(|path| {
                Ok(LogoFile {
                    url: Url::parse(TVAPP_LOGOS_URL)?.join(path)?,
                    cache_file: cache_dir().join("tvapp").join(path),
                })
            })
            .collect::<eyre::Result<_>>()?;
        let avm = self
            .avm_file_name
            .iter()
            .map(|file_name| {
                Ok(LogoFile {
                    url: Url::parse(AVM_LOGOS_URL)?.join(file_name)?,
                    cache_file: cache_dir().join(file_name),
                })
            })
            .collect::<eyre::Result<_>>()?;

        Ok([tvapp, avm])
    }
}

#[derive(Debug)]
struct LogoFile {
    url: Url,
    cache_file: PathBuf,
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

/// Returns the PNG data of the first logo found in `sources`.
///
/// Within each source, a cached logo is preferred over downloading a more
/// likely candidate that wasn't found before.
pub async fn load_logo(http: reqwest::Client, sources: LogoSources) -> eyre::Result<Vec<u8>> {
    for files in sources.files()? {
        for file in &files {
            if let Ok(data) = tokio::fs::read(&file.cache_file).await {
                return Ok(data);
            }
        }
        for file in &files {
            match download_logo(&http, file).await {
                Ok(data) => return Ok(data),
                Err(e) => tracing::debug!("failed to load logo {}: {e:?}", file.url),
            }
        }
    }

    eyre::bail!("no logo found")
}

/// Downloads and caches the logo `file`.
async fn download_logo(http: &reqwest::Client, file: &LogoFile) -> eyre::Result<Vec<u8>> {
    static DOWNLOADS: OnceLock<Semaphore> = OnceLock::new();
    /// URLs that didn't provide a logo, to avoid requesting them again on reload.
    static MISSING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

    let is_missing = || MISSING.lock().unwrap().contains(file.url.as_str());
    let set_missing = || MISSING.lock().unwrap().insert(file.url.to_string());

    if is_missing() {
        eyre::bail!("not found before");
    }

    let _permit = DOWNLOADS
        .get_or_init(|| Semaphore::new(MAX_PARALLEL_DOWNLOADS))
        .acquire()
        .await?;

    let response = http.get(file.url.clone()).send().await?;
    if response.status() == StatusCode::NOT_FOUND {
        set_missing();
        eyre::bail!("not found");
    }
    let data = response.error_for_status()?.bytes().await?;

    if !data.starts_with(PNG_SIGNATURE) {
        set_missing();
        eyre::bail!("logo is not a PNG image");
    }

    if let Err(e) = write_cache(&file.cache_file, &data).await {
        tracing::warn!("failed to cache logo {}: {e:?}", file.url);
    }

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
    fn prefers_tvapp_logos() {
        let index = LogoIndex::from_listing(LISTING);
        let sources = LogoSources::new("ZDF HD", ChannelList::Tv, &index);

        assert_eq!(
            sources,
            LogoSources {
                tvapp_paths: vec!["hd/zdf_hd.png".to_owned(), "zdf_hd.png".to_owned()],
                avm_file_name: Some("zdf.png".to_owned()),
            }
        );

        let [tvapp, avm] = sources.files().unwrap();
        assert_eq!(
            tvapp[0].url.as_str(),
            "https://tv.avm.de/tvapp/logos/hd/zdf_hd.png"
        );
        assert_eq!(tvapp[0].cache_file, cache_dir().join("tvapp/hd/zdf_hd.png"));
        assert_eq!(
            avm[0].url.as_str(),
            "https://download.avm.de/tv/logos/zdf.png"
        );
        assert_eq!(avm[0].cache_file, cache_dir().join("zdf.png"));

        assert!(LogoSources::new("!!!", ChannelList::Tv, &index).is_empty());
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
