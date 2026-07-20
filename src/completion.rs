use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::ValueEnum;
use clap_complete::env::{Bash, EnvCompleter, Fish, Zsh};
use tempfile::NamedTempFile;

/// 支持一键安装动态补全注册脚本的 shell。
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Fish,
    Zsh,
}

impl CompletionShell {
    fn env_completer(self) -> &'static dyn EnvCompleter {
        match self {
            Self::Bash => &Bash,
            Self::Fish => &Fish,
            Self::Zsh => &Zsh,
        }
    }

    fn script_file_name(self) -> &'static str {
        match self {
            Self::Bash => "dtools",
            Self::Fish => "dtools.fish",
            Self::Zsh => "_dtools",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Fish => "fish",
            Self::Zsh => "zsh",
        }
    }
}

/// 安装会在每次补全时调用当前 `PATH` 中 dtools 的动态注册脚本。
pub fn install(shell: CompletionShell) -> Result<()> {
    let destination = completion_install_path(shell)?;
    let script = completion_registration_script(shell)?;
    install_script(shell, &destination, &script)?;

    println!(
        "installed {} completion to {}",
        shell.name(),
        destination.display()
    );
    if matches!(shell, CompletionShell::Zsh) {
        let parent = destination
            .parent()
            .context("completion install target has no parent directory")?;
        println!(
            "if zsh does not pick it up automatically, add {} to `fpath` and rerun `compinit`",
            parent.display()
        );
    }
    Ok(())
}

fn completion_install_path(shell: CompletionShell) -> Result<PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from);
    let xdg_config_home = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let xdg_data_home = env::var_os("XDG_DATA_HOME").map(PathBuf::from);
    completion_install_path_from_env(
        shell,
        home.as_deref(),
        xdg_config_home.as_deref(),
        xdg_data_home.as_deref(),
    )
}

fn completion_install_path_from_env(
    shell: CompletionShell,
    home: Option<&Path>,
    xdg_config_home: Option<&Path>,
    xdg_data_home: Option<&Path>,
) -> Result<PathBuf> {
    let home = home.context("could not determine home directory for completion install")?;
    let path = match shell {
        CompletionShell::Fish => xdg_config_home
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.join(".config"))
            .join("fish/completions")
            .join(shell.script_file_name()),
        CompletionShell::Bash => xdg_data_home
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("bash-completion/completions")
            .join(shell.script_file_name()),
        CompletionShell::Zsh => xdg_data_home
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("zsh/site-functions")
            .join(shell.script_file_name()),
    };
    Ok(path)
}

fn completion_registration_script(shell: CompletionShell) -> Result<String> {
    if matches!(shell, CompletionShell::Fish) {
        return Ok(concat!(
            "function __dtools_complete\n",
            "    set -l dtools_bin (builtin type --force-path dtools)\n",
            "    test -n \"$dtools_bin\"; or return\n",
            "\n",
            "    COMPLETE=fish \"$dtools_bin\" -- ",
            "(commandline --current-process --tokenize --cut-at-cursor) ",
            "(commandline --current-token)\n",
            "end\n",
            "\n",
            "complete --keep-order --exclusive --command dtools ",
            "--arguments \"(__dtools_complete)\"\n",
        )
        .to_owned());
    }

    let mut buffer = Vec::new();
    shell
        .env_completer()
        .write_registration(
            "COMPLETE",
            "dtools",
            "dtools",
            "__DTOOLS_COMPLETER__",
            &mut buffer,
        )
        .context("failed to generate completion registration")?;
    let script = String::from_utf8(buffer)
        .context("generated completion registration was not valid UTF-8")?;

    Ok(match shell {
        CompletionShell::Bash => script
            .replace(
                "    local words=(\"${COMP_WORDS[@]}\")",
                concat!(
                    "    local dtools_bin\n",
                    "    dtools_bin=$(builtin type -P dtools) || return\n",
                    "    [[ -n \"$dtools_bin\" ]] || return\n",
                    "    local words=(\"${COMP_WORDS[@]}\")",
                ),
            )
            .replace("\"__DTOOLS_COMPLETER__\" --", "\"$dtools_bin\" --"),
        CompletionShell::Zsh => script
            .replace(
                "function _clap_dynamic_completer_dtools() {\n",
                concat!(
                    "function _clap_dynamic_completer_dtools() {\n",
                    "    local dtools_bin\n",
                    "    dtools_bin=\"$(builtin whence -p dtools)\" || return\n",
                    "    [[ -n \"$dtools_bin\" ]] || return\n",
                ),
            )
            .replace("__DTOOLS_COMPLETER__ --", "\"$dtools_bin\" --"),
        CompletionShell::Fish => unreachable!("fish completion is generated separately"),
    })
}

fn install_script(shell: CompletionShell, destination: &Path, script: &str) -> Result<()> {
    let parent = destination.parent().with_context(|| {
        format!(
            "completion install target has no parent directory: {}",
            destination.display()
        )
    })?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create completion directory {}", parent.display()))?;

    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create a temporary file in {}", parent.display()))?;
    temporary
        .write_all(script.as_bytes())
        .with_context(|| format!("failed to write {} completion", shell.name()))?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("failed to sync {} completion", shell.name()))?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)
        .with_context(|| {
            format!(
                "failed to install {} completion to {}",
                shell.name(),
                destination.display()
            )
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_xdg_completion_paths() {
        let home = Path::new("/home/tester");
        let config = Path::new("/custom/config");
        let data = Path::new("/custom/data");

        assert_eq!(
            completion_install_path_from_env(
                CompletionShell::Bash,
                Some(home),
                Some(config),
                Some(data)
            )
            .unwrap(),
            data.join("bash-completion/completions/dtools")
        );
        assert_eq!(
            completion_install_path_from_env(
                CompletionShell::Fish,
                Some(home),
                Some(config),
                Some(data)
            )
            .unwrap(),
            config.join("fish/completions/dtools.fish")
        );
        assert_eq!(
            completion_install_path_from_env(
                CompletionShell::Zsh,
                Some(home),
                Some(config),
                Some(data)
            )
            .unwrap(),
            data.join("zsh/site-functions/_dtools")
        );
    }

    #[test]
    fn generated_scripts_request_dynamic_completions() {
        for shell in [
            CompletionShell::Bash,
            CompletionShell::Fish,
            CompletionShell::Zsh,
        ] {
            let script = completion_registration_script(shell).unwrap();

            assert!(script.contains("COMPLETE="));
            assert!(script.contains(shell.name()));
            assert!(script.contains("dtools_bin"));
            assert!(!script.contains("__DTOOLS_COMPLETER__"));
        }
    }
}
