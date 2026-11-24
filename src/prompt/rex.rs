use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::{cache_root, capture, fingerprint, repaint, write_sample};
use crate::AnyResult;

const INTERVAL: Duration = Duration::from_secs(1);

struct Pane {
    server: OsString,
    block: String,
    cache: PathBuf,
}

impl Pane {
    fn current() -> Option<Self> {
        let server = env::var_os("REX_SERVER").filter(|server| !server.is_empty())?;
        let block = env::var("REX_BLOCK").ok()?;
        if !block.starts_with("block:") {
            return None;
        }
        let cache = cache_root()?
            .join("dcx/rex-v1")
            .join(format!("{:016x}", fingerprint(&(&server, &block))));
        Some(Self {
            server,
            block,
            cache,
        })
    }

    fn remote(&self) -> bool {
        let Some((status, output)) =
            capture(Command::new("rex").arg("--server").arg(&self.server).args([
                "--autostart=false",
                "--timeout=1s",
                "session",
                "inspect",
                &self.block,
                "--json",
            ]))
        else {
            return false;
        };
        status.success()
            && serde_json::from_reader(output.take(8 * 1024 * 1024))
                .is_ok_and(|session| remote_owner(&session, &self.block))
    }
}

pub(super) struct State {
    pub remote: bool,
    updated: Option<SystemTime>,
}

impl State {
    pub(super) fn read() -> Option<Self> {
        Some(Self::read_from(&Pane::current()?.cache))
    }

    fn read_from(cache: &Path) -> Self {
        let file = File::open(cache.join("state")).ok();
        let updated = file
            .as_ref()
            .and_then(|file| file.metadata().ok())
            .and_then(|metadata| metadata.modified().ok());
        let mut value = String::new();
        let remote = file.is_some_and(|file| {
            file.take(8).read_to_string(&mut value).is_ok() && value == "remote"
        });
        Self { remote, updated }
    }

    pub(super) fn stale(&self) -> bool {
        self.updated
            .and_then(|time| time.elapsed().ok())
            .is_none_or(|age| age >= INTERVAL)
    }
}

pub(super) fn refresh(shell_pid: u32, shown_remote: bool) -> AnyResult {
    let Some(pane) = Pane::current() else {
        return Ok(());
    };
    fs::create_dir_all(&pane.cache)?;
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(pane.cache.join("refresh.lock"))?;
    lock.lock()?;
    let state = State::read_from(&pane.cache);
    let remote = if state.stale() {
        let remote = pane.remote();
        write_sample(
            &pane.cache.join("state"),
            if remote { "remote" } else { "local" },
        )?;
        remote
    } else {
        state.remote
    };
    drop(lock);
    if remote != shown_remote {
        repaint(shell_pid);
    }
    Ok(())
}

fn remote_owner(session: &Value, block: &str) -> bool {
    let Some(windows) = session["windows"].as_array() else {
        return false;
    };
    let owner = windows
        .iter()
        .filter_map(|window| window["layers"].as_array())
        .flatten()
        .filter_map(|layer| layer["blocks"].as_array())
        .flatten()
        .find(|pane| pane["block_id"].as_str() == Some(block))
        .and_then(|pane| pane["owner"].as_str());
    let Some(owner) = owner else {
        return false;
    };
    session["clients"].as_array().is_some_and(|clients| {
        clients.iter().any(|client| {
            client["client_id"].as_str() == Some(owner)
                && client["connection_state"].as_str() == Some("connected")
                && client["principal"]["transport"].as_str() == Some("tailscale")
        })
    })
}
