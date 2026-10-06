// SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Playback of FRITZ!Box channels with mpv.
//!
//! The FRITZ!Box only serves one stream per client, so there is at most one
//! mpv process at a time: starting a new channel terminates the previous
//! process first (mpv then sends an RTSP TEARDOWN to free the tuner).

use std::{cell::RefCell, ffi::OsStr, time::Duration};

use adw::{gio, glib, prelude::*};
use futures_util::future::{select, Either};
use gettextrs::gettext;

use crate::{application::TvApplication, settings::TvSettings, utils::spawn, window::TvWindow};

pub const MPV: &str = "mpv";

/// SIGTERM, lets mpv shut down gracefully.
const SIGTERM: i32 = 15;
const TERMINATE_TIMEOUT: Duration = Duration::from_secs(3);
/// Number of output bytes kept for error messages.
const OUTPUT_TAIL_SIZE: usize = 8 * 1024;
/// mpv exit code when quitting due to a signal or the quit key bindings.
const EXIT_CODE_QUIT: i32 = 4;

thread_local! {
    static CURRENT: RefCell<Option<gio::Subprocess>> = const { RefCell::new(None) };
}

pub fn is_installed() -> bool {
    glib::find_program_in_path(MPV).is_some()
}

/// Plays `uri` in a new mpv process, replacing the currently running one.
pub async fn play(title: &str, uri: &str) -> eyre::Result<()> {
    stop().await;

    if !is_installed() {
        eyre::bail!(gettext(
            "mpv is not installed. FritzTV needs the mpv video player."
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

    CURRENT.with(|current| current.replace(Some(process.clone())));
    spawn(watch(process, title.to_owned()));

    Ok(())
}

/// Terminates the running mpv process and waits until it has exited.
pub async fn stop() {
    let Some(process) = CURRENT.with(|current| current.take()) else {
        return;
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
}

/// Terminates the running mpv process without waiting, e.g. on shutdown.
pub fn terminate() {
    if let Some(process) = CURRENT.with(|current| current.take()) {
        process.send_signal(SIGTERM);
    }
}

fn is_current(process: &gio::Subprocess) -> bool {
    CURRENT.with(|current| current.borrow().as_ref() == Some(process))
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
    CURRENT.with(|current| current.take());

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
                    // translators: `{channel}` is replaced by the channel name, `{error}` by the error message of mpv
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
