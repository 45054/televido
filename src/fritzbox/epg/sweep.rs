// SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Keeps the programme guide up to date by visiting the multiplexes while
//! the tuner is idle.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use tokio::{
    sync::{mpsc::UnboundedSender, watch},
    time::{sleep, Instant},
};

use super::{
    mux,
    rtsp::StatusError,
    scan::{self, ScanError},
    Inner,
};
use crate::fritzbox::FritzChannel;

/// Timing of the sweep.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// Pause after playback ended before the tuner is used again.
    pub settle: Duration,
    /// How often is checked whether a multiplex needs to be scanned.
    pub poll: Duration,
    /// Pause before a multiplex that failed is tried again.
    pub retry: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            settle: Duration::from_secs(3),
            poll: Duration::from_secs(60),
            retry: Duration::from_secs(5 * 60),
        }
    }
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

/// Resolves when the sender of `cancel` was dropped.
async fn cancelled(cancel: &mut watch::Receiver<()>) {
    while cancel.changed().await.is_ok() {}
}

fn is_cancelled(cancel: &watch::Receiver<()>) -> bool {
    cancel.has_changed().is_err()
}

/// Scans the multiplexes of `channels` until `cancel` is dropped.
pub async fn run(
    inner: Arc<Inner>,
    channels: Vec<FritzChannel>,
    mut cancel: watch::Receiver<()>,
    notify: UnboundedSender<()>,
) {
    inner.load_cache().await;
    let _ = notify.send(());

    let muxes = mux::group(&channels);
    let timing = inner.timing;
    // multiplexes that failed, and when to try them again
    let mut retry_at: HashMap<String, Instant> = HashMap::new();
    // multiplexes that only work with the full list of PIDs
    let mut full_pids: HashSet<String> = HashSet::new();

    'sweep: loop {
        for mux in &muxes {
            if is_cancelled(&cancel) {
                return;
            }

            // let playback use the tuner
            while inner.tuner.is_playing() {
                tokio::select! {
                    _ = cancelled(&mut cancel) => return,
                    _ = inner.tuner.playback_ended() => {}
                }
                tokio::select! {
                    _ = cancelled(&mut cancel) => return,
                    _ = sleep(timing.settle) => {}
                }
            }

            if retry_at
                .get(&mux.key)
                .is_some_and(|retry| *retry > Instant::now())
            {
                continue;
            }
            if !inner.store().needs_scan(mux, unix_now()) {
                continue;
            }

            let url = if full_pids.contains(&mux.key) {
                &mux.fallback_url
            } else {
                &mux.scan_url
            };
            tracing::debug!("reading the programme guide of {}", mux.key);

            let guard = inner.tuner.lock_session().await;
            if inner.tuner.is_playing() {
                continue 'sweep;
            }
            let result = {
                let mut cancel = cancel.clone();
                scan::scan(url, mux.channel_pids().collect(), async {
                    tokio::select! {
                        _ = inner.tuner.playback_started() => {}
                        _ = cancelled(&mut cancel) => {}
                    }
                })
                .await
            };
            drop(guard);

            match result {
                Ok(scan) => {
                    let now = unix_now();
                    if scan.pat.is_none() {
                        // maybe the FRITZ!Box only starts the stream if it
                        // contains audio or video
                        if !full_pids.contains(&mux.key) && mux.scan_url != mux.fallback_url {
                            tracing::info!(
                                "received no service information from {}, trying again with all PIDs",
                                mux.key
                            );
                            full_pids.insert(mux.key.clone());
                            continue 'sweep;
                        }
                        tracing::warn!("received no service information from {}", mux.key);
                        retry_at.insert(mux.key.clone(), Instant::now() + timing.retry);
                    }
                    tracing::debug!(
                        "{}: {} services, {} EIT sections",
                        mux.key,
                        scan.pat.as_ref().map_or(0, |pat| pat.programs.len()),
                        scan.eit.len()
                    );
                    {
                        let mut store = inner.store();
                        store.add_scan(mux, &scan, now);
                        store.prune(now);
                    }
                    inner.save_cache().await;
                    let _ = notify.send(());
                }
                // playback started or the sweep was cancelled
                Err(ScanError::Stopped) => continue 'sweep,
                Err(ScanError::Failed(e)) => {
                    let rejected = e.downcast_ref::<StatusError>().is_some();
                    if rejected && !full_pids.contains(&mux.key) && mux.scan_url != mux.fallback_url
                    {
                        tracing::info!(
                            "{} rejected the stream without audio and video, trying again with all PIDs: {e:#}",
                            mux.key
                        );
                        full_pids.insert(mux.key.clone());
                        continue 'sweep;
                    }
                    tracing::warn!("failed to read the programme guide of {}: {e:#}", mux.key);
                    retry_at.insert(mux.key.clone(), Instant::now() + timing.retry);
                }
            }
        }

        tokio::select! {
            _ = cancelled(&mut cancel) => return,
            _ = sleep(timing.poll) => {}
        }
    }
}
