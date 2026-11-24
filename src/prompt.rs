use std::env;
use std::fs::{self, File};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::{Args, Subcommand, ValueEnum};
use tempfile::NamedTempFile;

use crate::AnyResult;

mod layout;
mod probes;
mod rex;

const GIT_INTERVAL: Duration = Duration::from_secs(2);
const TOOL_INTERVAL: Duration = Duration::from_secs(30);
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
const MAX_SAMPLE: u64 = 8192;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Shell {
    Fish,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Print the shell integration script")]
    Init { shell: Shell },
    #[command(hide = true)]
    Render(RenderArguments),
    #[command(hide = true)]
    Refresh {
        #[arg(long)]
        shell_pid: u32,
        #[arg(long)]
        shown: u64,
        #[arg(long)]
        after_command: Option<u64>,
    },
    #[command(hide = true)]
    RefreshRex {
        #[arg(long)]
        shell_pid: u32,
        #[arg(long)]
        shown_remote: bool,
    },
}

#[derive(Debug, Args)]
pub struct RenderArguments {
    #[arg(long)]
    shell_pid: u32,
    #[arg(long)]
    status: u32,
    #[arg(long)]
    duration: u64,
    #[arg(long)]
    jobs: usize,
    #[arg(long)]
    mode: Option<String>,
    #[arg(long)]
    identity: Option<String>,
    #[arg(long)]
    hostname: Option<String>,
    #[arg(long, default_value_t = 80)]
    width: usize,
    #[arg(long)]
    after_command: bool,
}

pub fn run(command: Command) -> AnyResult {
    match command {
        Command::Init { shell: Shell::Fish } => init_fish()?,
        Command::Render(arguments) => render(arguments)?,
        Command::Refresh {
            shell_pid,
            shown,
            after_command,
        } => refresh(shell_pid, shown, after_command)?,
        Command::RefreshRex {
            shell_pid,
            shown_remote,
        } => rex::refresh(shell_pid, shown_remote)?,
    }
    Ok(())
}

fn init_fish() -> AnyResult {
    let executable = env::current_exe()?;
    let path = executable
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("dcx path is not UTF-8"))?;
    // Fish 单引号内只转义反斜线和单引号，路径可包含空格及 shell 符号。
    let quoted = path.replace('\\', "\\\\").replace('\'', "\\'");
    println!("set -l __dcx_prompt_executable '{quoted}'");
    print!("{}", include_str!("prompt/fish.fish"));
    Ok(())
}

fn render(arguments: RenderArguments) -> AnyResult {
    let RenderArguments {
        shell_pid,
        status,
        duration,
        jobs,
        mode,
        identity,
        hostname,
        width,
        after_command,
    } = arguments;
    let cwd = env::current_dir()?;
    let cache = cache_dir(&cwd);
    let now = timestamp();
    let after_command = after_command.then_some(now);
    let samples = cache.as_deref().map(Samples::read).unwrap_or_default();
    let ssh = ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
        .iter()
        .any(|key| env::var_os(key).is_some());
    let rex = hostname
        .as_ref()
        .filter(|_| !ssh)
        .and_then(|_| rex::State::read());
    let mut layout = layout::Layout::new(width);
    if let Some(identity) = identity {
        layout.identity(&identity);
    }
    if let Some(hostname) = hostname
        .as_deref()
        .filter(|_| ssh || rex.as_ref().is_some_and(|state| state.remote))
    {
        layout.hostname(hostname.split('.').next().unwrap_or_default());
    }
    let git_depth = samples
        .0
        .iter()
        .zip(probes::ALL)
        .find(|(_, probe)| probe.name == "git")
        .and_then(|(sample, _)| sample.text.split_once('\n'))
        .and_then(|(depth, _)| depth.parse::<usize>().ok());
    layout.directory(&cwd, git_depth);
    let environment = samples
        .0
        .iter()
        .zip(probes::ALL)
        .find(|(_, probe)| probe.name == "environment")
        .map_or("", |(sample, _)| sample.text.as_str());
    for (sample, probe) in samples.0.iter().zip(probes::ALL) {
        if layout.nerd() && probe.name == "python" {
            layout.python(&sample.text, environment);
        } else if !sample.text.is_empty() && !(layout.nerd() && probe.name == "environment") {
            layout.module(probe.name, &sample.text);
        }
    }
    if !layout.nerd() {
        layout.jobs(jobs);
    }
    layout.duration(duration);
    let mut stdout = io::stdout().lock();
    write!(stdout, "{}", layout.finish(status, mode.as_deref(), jobs))?;
    stdout.flush()?;

    if cache.is_some() && samples.needs_refresh(now, after_command) {
        let mut args = vec![
            "refresh".to_owned(),
            format!("--shell-pid={shell_pid}"),
            format!("--shown={}", samples.fingerprint()),
        ];
        if let Some(after_command) = after_command {
            args.push(format!("--after-command={after_command}"));
        }
        spawn_worker(&args);
    }
    if let Some(state) = rex.filter(|state| state.stale()) {
        let mut args = vec!["refresh-rex".to_owned(), format!("--shell-pid={shell_pid}")];
        if state.remote {
            args.push("--shown-remote".into());
        }
        spawn_worker(&args);
    }
    Ok(())
}

fn refresh(shell_pid: u32, shown: u64, after_command: Option<u64>) -> AnyResult {
    let cwd = env::current_dir()?;
    let Some(cache) = cache_dir(&cwd) else {
        return Ok(());
    };
    fs::create_dir_all(&cache)?;
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(cache.join("refresh.lock"))?;
    // 多个 Fish 共用缓存。等待同一轮探测的会话也需要收到重绘通知。
    lock.lock()?;
    let samples = Samples::read(&cache);
    let now = timestamp();
    let sources = probes::source_kinds(&cwd);
    let selection = format!("{:016x}", probes::selection_fingerprint(&cwd, sources));
    let selection_path = cache.join("selection");
    let selection_changed = fs::read_to_string(&selection_path).ok().as_deref() != Some(&selection);
    thread::scope(|scope| {
        for (probe, sample) in probes::ALL.iter().zip(samples.0) {
            if stale(probe, &sample, now, after_command)
                || (selection_changed && probe.name != "git")
            {
                let cache = &cache;
                let cwd = &cwd;
                scope.spawn(move || {
                    let started = timestamp();
                    let value = probes::collect(probe, cwd, sources).unwrap_or(sample.text);
                    let _ = write_sample(
                        &cache.join(probe.name),
                        &format!("started {started}\n{value}"),
                    );
                });
            }
        }
    });
    if selection_changed {
        let _ = write_sample(&selection_path, &selection);
    }
    let changed = Samples::read(&cache).fingerprint() != shown;
    drop(lock);
    if changed {
        repaint(shell_pid);
    }
    Ok(())
}

fn spawn_worker(args: &[String]) {
    if let Ok(executable) = env::current_exe() {
        let _ = process::Command::new(executable)
            .arg("prompt")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

fn repaint(shell_pid: u32) {
    if shell_pid > 1 && cfg!(unix) {
        let _ = process::Command::new("/bin/kill")
            .args(["-s", "USR1", &shell_pid.to_string()])
            .status();
    }
}

fn fingerprint(value: &impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn cache_root() -> Option<PathBuf> {
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
}

fn cache_dir(cwd: &Path) -> Option<PathBuf> {
    let root = cache_root()?;
    let mut selectors = env::vars_os()
        .filter(|(key, _)| {
            let key = key.to_string_lossy();
            matches!(
                key.as_ref(),
                "PATH"
                    | "PWD"
                    | "DCX_PROMPT_STYLE"
                    | "VIRTUAL_ENV"
                    | "CONDA_PREFIX"
                    | "CONDA_DEFAULT_ENV"
                    | "XDG_CONFIG_HOME"
                    | "PYTHONHOME"
                    | "JAVA_HOME"
                    | "RBENV_VERSION"
                    | "RUBY_VERSION"
                    | "PIXI_ENVIRONMENT_NAME"
            ) || [
                "MISE_", "RUSTUP_", "PYENV_", "ASDF_", "NVM_", "VOLTA_", "GIT_", "GO",
            ]
            .iter()
            .any(|prefix| key.starts_with(prefix))
        })
        .collect::<Vec<_>>();
    selectors.sort_unstable();
    Some(
        root.join("dcx/prompt-v1")
            .join(format!("{:016x}", fingerprint(&(cwd, selectors)))),
    )
}

#[derive(Default)]
struct Sample {
    updated: u64,
    text: String,
}

#[derive(Default)]
struct Samples([Sample; probes::ALL.len()]);

impl Samples {
    fn read(cache: &Path) -> Self {
        Self(probes::ALL.map(|probe| {
            let path = cache.join(probe.name);
            let Ok(file) = File::open(path) else {
                return Sample::default();
            };
            let modified = file
                .metadata()
                .and_then(|metadata| metadata.modified())
                .map(nanoseconds)
                .unwrap_or_default();
            let mut text = String::new();
            let _ = file.take(MAX_SAMPLE).read_to_string(&mut text);
            if let Some((started, value)) = text
                .strip_prefix("started ")
                .and_then(|sample| sample.split_once('\n'))
                && let Ok(updated) = started.parse()
            {
                return Sample {
                    updated,
                    text: value.to_owned(),
                };
            }
            Sample {
                updated: modified,
                text,
            }
        }))
    }

    fn fingerprint(&self) -> u64 {
        fingerprint(&self.0.each_ref().map(|sample| &sample.text))
    }

    fn needs_refresh(&self, now: u64, after_command: Option<u64>) -> bool {
        probes::ALL
            .iter()
            .zip(&self.0)
            .any(|(probe, sample)| stale(probe, sample, now, after_command))
    }
}

fn stale(probe: &probes::Probe, sample: &Sample, now: u64, after_command: Option<u64>) -> bool {
    let interval = if probe.name == "git" {
        if after_command.is_some_and(|command| command > sample.updated) {
            return true;
        }
        GIT_INTERVAL
    } else {
        TOOL_INTERVAL
    };
    now.saturating_sub(sample.updated) >= interval.as_nanos() as u64
}

fn write_sample(path: &Path, value: &str) -> io::Result<()> {
    let mut file = NamedTempFile::new_in(path.parent().expect("cache file has a parent"))?;
    file.write_all(value.as_bytes())?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn timestamp() -> u64 {
    nanoseconds(SystemTime::now())
}

fn nanoseconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

fn clean(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .collect()
}

fn capture(command: &mut process::Command) -> Option<(process::ExitStatus, File)> {
    capture_output(command, ProbeStderr::Discard)
}

enum ProbeStderr<'a> {
    Discard,
    Merge,
    Separate(&'a File),
}

fn capture_output(
    command: &mut process::Command,
    stderr: ProbeStderr<'_>,
) -> Option<(process::ExitStatus, File)> {
    // 临时文件避免管道写满阻塞，也避免超时后等待后代进程关闭管道。
    let mut output = tempfile::tempfile().ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let stderr_stdio = match stderr {
        ProbeStderr::Discard => Stdio::null(),
        ProbeStderr::Merge => Stdio::from(output.try_clone().ok()?),
        ProbeStderr::Separate(file) => Stdio::from(file.try_clone().ok()?),
    };
    let mut child = command
        .stdin(Stdio::null())
        .stderr(stderr_stdio)
        .stdout(output.try_clone().ok()?)
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                output.seek(SeekFrom::Start(0)).ok()?;
                if let ProbeStderr::Separate(mut file) = stderr {
                    file.seek(SeekFrom::Start(0)).ok()?;
                }
                return Some((status, output));
            }
            Ok(None) if started.elapsed() < PROBE_TIMEOUT => {
                thread::sleep(Duration::from_millis(5));
            }
            _ => {
                // Shim 可能 fork 解释器。超时时同时结束该次探测的进程组。
                #[cfg(unix)]
                let _ = process::Command::new("/bin/kill")
                    .args(["-KILL", "--", &format!("-{}", child.id())])
                    .stderr(Stdio::null())
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}
