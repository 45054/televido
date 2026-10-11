// SPDX-FileCopyrightText: 2026 Televido contributors
// SPDX-FileContributor: Claude (Anthropic)
// SPDX-License-Identifier: GPL-3.0-or-later

//! Video playback with mpv.
//!
//! There is at most one mpv process at a time: playing a new video terminates
//! the previous process first. This matters for FRITZ!Box channels, because
//! the FRITZ!Box only serves one stream per client (mpv sends an RTSP TEARDOWN
//! on exit to free the tuner). For the same reason, reading the programme
//! guide is paused while a FRITZ!Box channel is playing, see
//! [`crate::fritzbox::Epg`].

use std::{cell::RefCell, ffi::OsStr, time::Duration};

use adw::{gio, glib, prelude::*};
use futures_util::future::{select, Either};
use gettextrs::gettext;

use crate::{
    application::TvApplication,
    settings::TvSettings,
    utils::{spawn, tokio},
    window::TvWindow,
};

pub const MPV: &str = "mpv";

/// SIGTERM, lets mpv shut down gracefully.
const SIGTERM: i32 = 15;
const TERMINATE_TIMEOUT: Duration = Duration::from_secs(3);
/// Number of output bytes kept for error messages.
const OUTPUT_TAIL_SIZE: usize = 8 * 1024;
/// mpv exit code when quitting due to a signal or the quit key bindings.
const EXIT_CODE_QUIT: i32 = 4;

/// The running mpv process.
struct Current {
    process: gio::Subprocess,
    /// Whether it plays a FRITZ!Box channel and therefore holds the tuner.
    uses_tuner: bool,
}

thread_local! {
    static CURRENT: RefCell<Option<Current>> = const { RefCell::new(None) };
}

pub fn is_installed() -> bool {
    glib::find_program_in_path(MPV).is_some()
}

/// Plays `uri` in a new mpv process, replacing the currently running one.
///
/// `uses_tuner` must be set for FRITZ!Box channels.
pub async fn play(title: &str, uri: &str, uses_tuner: bool) -> eyre::Result<()> {
    let tuner = TvApplication::get().fritzbox().epg().tuner();

    if uses_tuner {
        // the programme guide needs the tuner, too
        let tuner = tuner.clone();
        tokio(async move { tuner.begin_playback().await }).await;
        stop().await;
    } else if stop().await {
        tuner.end_playback();
    }

    let result = start(title, uri, uses_tuner);
    if result.is_err() && uses_tuner {
        tuner.end_playback();
    }
    result
}

fn start(title: &str, uri: &str, uses_tuner: bool) -> eyre::Result<()> {
    if !is_installed() {
        eyre::bail!(gettext(
            "mpv is not installed. Televido needs the mpv video player."
        ));
    }

    let arguments = TvSettings::get().mpv_arguments();
    let title_argument = format!("--force-media-title={title}");

    let argv: Vec<&OsStr> = [MPV]
        .into_iter()
        .chain(
            arguments
                .iter()
                .map(|argument| argument.trim())
                .filter(|argument| !argument.is_empty()),
        )
        .chain([title_argument.as_str(), "--", uri])
        .map(OsStr::new)
        .collect();

    tracing::debug!("starting {argv:?}");

    let process = gio::Subprocess::newv(
        &argv,
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_MERGE,
    )
    .map_err(|e| eyre::eyre!(e).wrap_err(gettext("Failed to start mpv")))?;

    CURRENT.with(|current| {
        current.replace(Some(Current {
            process: process.clone(),
            uses_tuner,
        }))
    });
    spawn(watch(process, title.to_owned()));

    Ok(())
}

/// Terminates the running mpv process and waits until it has exited.
///
/// Returns whether the terminated process held the FRITZ!Box tuner.
async fn stop() -> bool {
    let Some(Current {
        process,
        uses_tuner,
    }) = CURRENT.with(|current| current.take())
    else {
        return false;
    };

    process.send_signal(SIGTERM);

    let exited = select(
        Box::pin(process.wait_future()),
        Box::pin(glib::timeout_future(TERMINATE_TIMEOUT)),
    )
    .await;

    if let Either::Right(_) = exited {
        tracing::warn!("mpv did not exit after SIGTERM, killing it");
        process.force_exit();
        let _ = process.wait_future().await;
    }

    uses_tuner
}

/// Terminates the running mpv process without waiting, e.g. on shutdown.
pub fn terminate() {
    if let Some(current) = CURRENT.with(|current| current.take()) {
        current.process.send_signal(SIGTERM);
    }
}

fn is_current(process: &gio::Subprocess) -> bool {
    CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .is_some_and(|current| &current.process == process)
    })
}

/// Keeps the application alive while mpv is running and reports errors.
async fn watch(process: gio::Subprocess, title: String) {
    let _hold = TvApplication::get().hold();

    // Always drain the output, otherwise mpv blocks once the pipe is full.
    let mut tail: Vec<u8> = Vec::new();
    if let Some(stdout) = process.stdout_pipe() {
        loop {
            match stdout
                .read_bytes_future(4096, glib::Priority::DEFAULT)
                .await
            {
                Ok(bytes) if !bytes.is_empty() => {
                    tail.extend_from_slice(&bytes);
                    if tail.len() > OUTPUT_TAIL_SIZE {
                        tail.drain(..tail.len() - OUTPUT_TAIL_SIZE);
                    }
                }
                _ => break,
            }
        }
    }

    let _ = process.wait_future().await;

    // terminated on purpose by `stop()` or `terminate()`
    if !is_current(&process) {
        return;
    }
    let uses_tuner = CURRENT
        .with(|current| current.take())
        .is_some_and(|current| current.uses_tuner);
    if uses_tuner {
        TvApplication::get().fritzbox().epg().tuner().end_playback();
    }

    if !process.has_exited() {
        return;
    }
    let status = process.exit_status();
    if status == 0 || status == EXIT_CODE_QUIT {
        return;
    }

    let output = String::from_utf8_lossy(&tail);
    tracing::error!("mpv exited with status {status}:\n{output}");

    let reason = output
        .lines()
        .flat_map(|line| line.split('\r'))
        .map(str::trim)
        .rfind(|line| !line.is_empty() && !line.starts_with("Exiting..."))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("exit status {status}"));

    if let Some(window) = TvApplication::get()
        .active_window()
        .and_downcast::<TvWindow>()
    {
        window.add_toast(
            adw::Toast::builder()
                .title(
                    // translators: `{channel}` is replaced by the channel name or the title of the show, `{error}` by the error message of mpv
                    gettext("mpv could not play “{channel}”: {error}")
                        .replace("{channel}", &title)
                        .replace("{error}", &reason),
                )
                .use_markup(false)
                .timeout(10)
                .build(),
        );
    }
}
