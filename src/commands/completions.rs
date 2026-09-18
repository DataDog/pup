use anyhow::{Context, Result};
use clap_complete::Shell;
use std::fs;
use std::path::PathBuf;

use crate::cli_command;

/// Generate completions to stdout (the raw clap output).
pub fn generate(shell: Shell) {
    clap_complete::generate(shell, &mut cli_command(), "pup", &mut std::io::stdout());
}

/// Install a dynamic loader script for the given shell.
///
/// Instead of writing the static completion output, this writes a thin wrapper
/// that checks `pup --version` at shell startup and regenerates the cached
/// completions automatically whenever pup is updated. Users never need to
/// re-run `--install` after upgrading pup.
pub fn install(shell: Shell) -> Result<()> {
    let (loader_path, loader_content, post_install_msg) = loader_for(shell)?;

    if let Some(parent) = loader_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    fs::write(&loader_path, loader_content)
        .with_context(|| format!("failed to write: {}", loader_path.display()))?;

    println!("Installed {} loader to: {}", shell, loader_path.display());
    println!("{post_install_msg}");

    Ok(())
}

/// Returns (loader_path, loader_script, post_install_instructions).
///
/// The loader script is a tiny shell snippet. On each shell startup it:
///   1. Reads `pup --version`
///   2. Compares it to a cached version stamp
///   3. Regenerates the completion cache if pup was updated
///   4. Sources the cache
///
/// Cache lives at `$XDG_CACHE_HOME/pup/completions.<ext>` (typically
/// `~/.cache/pup/completions.<ext>`), separate from the loader so the
/// loader file itself never changes after install.
fn loader_for(shell: Shell) -> Result<(PathBuf, String, String)> {
    match shell {
        Shell::Bash => {
            let loader_path = bash_completion_dir()?.join("pup");
            let content = concat!(
                "# pup shell completions — managed by 'pup completions bash --install'\n",
                "# Automatically refreshes when pup is updated. Do not edit.\n",
                "__pup_load() {\n",
                "    local cache=\"${XDG_CACHE_HOME:-$HOME/.cache}/pup/completions.bash\"\n",
                "    local stamp; stamp=\"$(pup --version 2>/dev/null)\" || return 0\n",
                "    if [[ \"$(head -1 \"$cache\" 2>/dev/null)\" != \"# $stamp\" ]]; then\n",
                "        mkdir -p \"$(dirname \"$cache\")\"\n",
                "        { printf '# %s\\n' \"$stamp\"; pup completions bash; } \\\n",
                "            > \"$cache\" 2>/dev/null || return 0\n",
                "    fi\n",
                "    # shellcheck source=/dev/null\n",
                "    source \"$cache\" 2>/dev/null\n",
                "}\n",
                "__pup_load\n",
                "unset -f __pup_load\n",
            );
            let msg = format!(
                "Add the following to your ~/.bashrc (if not already present):\n  source \"{}\"",
                loader_path.display()
            );
            Ok((loader_path, content.to_string(), msg))
        }

        Shell::Zsh => {
            let dir = zsh_completion_dir();
            let loader_path = dir.join("_pup");
            // This file is autoloaded by zsh as the _pup completion function.
            // Its body checks the version, regenerates the cache if needed,
            // sources it (which redefines _pup with the real completion function),
            // then forwards the call.
            let content = concat!(
                "#compdef pup\n",
                "# pup shell completions — managed by 'pup completions zsh --install'\n",
                "# Automatically refreshes when pup is updated. Do not edit.\n",
                "local _pup_cache=\"${XDG_CACHE_HOME:-$HOME/.cache}/pup/completions.zsh\"\n",
                "local _pup_stamp; _pup_stamp=\"$(pup --version 2>/dev/null)\" || return 1\n",
                "if [[ \"$(head -1 \"$_pup_cache\" 2>/dev/null)\" != \"# $_pup_stamp\" ]]; then\n",
                "    mkdir -p \"${_pup_cache:h}\"\n",
                "    { printf '# %s\\n' \"$_pup_stamp\"; pup completions zsh; } \\\n",
                "        >| \"$_pup_cache\" 2>/dev/null || return 1\n",
                "fi\n",
                "# Source the cache — this redefines _pup with the real completion function.\n",
                "source \"$_pup_cache\" 2>/dev/null || return 1\n",
                "_pup \"$@\"\n",
            );
            let msg = format!(
                "Add the following to your ~/.zshrc (if not already present):\n  fpath=(\"{dir}\" $fpath)\n  autoload -Uz compinit && compinit",
                dir = dir.display()
            );
            Ok((loader_path, content.to_string(), msg))
        }

        Shell::Fish => {
            // Fish calls completion scripts at tab-press time, so we can check
            // the version and reload dynamically without any startup cost.
            let loader_path = fish_completion_dir()?.join("pup.fish");
            let content = concat!(
                "# pup shell completions — managed by 'pup completions fish --install'\n",
                "# Automatically refreshes when pup is updated. Do not edit.\n",
                "set -l __pup_cache \"$HOME/.cache/pup/completions.fish\"\n",
                "set -l __pup_stamp (pup --version 2>/dev/null)\n",
                "\n",
                "if test -n \"$__pup_stamp\"\n",
                "    set -l __pup_cached_stamp \"\"\n",
                "    if test -f \"$__pup_cache.version\"\n",
                "        set __pup_cached_stamp (cat \"$__pup_cache.version\" 2>/dev/null)\n",
                "    end\n",
                "    if test \"$__pup_cached_stamp\" != \"$__pup_stamp\"; or not test -f \"$__pup_cache\"\n",
                "        mkdir -p (dirname $__pup_cache)\n",
                "        pup completions fish > $__pup_cache 2>/dev/null\n",
                "        and echo $__pup_stamp > \"$__pup_cache.version\"\n",
                "    end\n",
                "    if test -f \"$__pup_cache\"\n",
                "        source $__pup_cache\n",
                "    end\n",
                "end\n",
            );
            // Fish auto-loads from its completions dir; no extra config needed.
            let msg = "Fish completions are active immediately — no further configuration needed."
                .to_string();
            Ok((loader_path, content.to_string(), msg))
        }

        Shell::PowerShell => {
            let loader_path = powershell_loader_path()?;
            let content = concat!(
                "# pup shell completions — managed by 'pup completions powershell --install'\n",
                "# Automatically refreshes when pup is updated. Do not edit.\n",
                "& {\n",
                "    $cache     = Join-Path $env:LOCALAPPDATA \"pup\\completions.ps1\"\n",
                "    $stampFile = \"$cache.version\"\n",
                "    $stamp     = (pup --version 2>$null)\n",
                "\n",
                "    if ($stamp -and (\n",
                "        !(Test-Path $cache) -or\n",
                "        !(Test-Path $stampFile) -or\n",
                "        (Get-Content $stampFile -Raw).Trim() -ne $stamp\n",
                "    )) {\n",
                "        New-Item -ItemType Directory -Force -Path (Split-Path $cache) | Out-Null\n",
                "        pup completions powershell | Out-File -FilePath $cache -Encoding UTF8\n",
                "        $stamp | Out-File -FilePath $stampFile -Encoding UTF8 -NoNewline\n",
                "    }\n",
                "    if (Test-Path $cache) { . $cache }\n",
                "}\n",
            );
            let msg = format!(
                "Add the following to your PowerShell profile ($PROFILE):\n  . \"{}\"",
                loader_path.display()
            );
            Ok((loader_path, content.to_string(), msg))
        }

        Shell::Elvish => {
            let dir = elvish_completion_dir()?;
            let loader_path = dir.join("pup.elv");
            let content = concat!(
                "# pup shell completions — managed by 'pup completions elvish --install'\n",
                "# Automatically refreshes when pup is updated. Do not edit.\n",
                "var cache = ~/.cache/pup/completions.elv\n",
                "\n",
                "var stamp = (pup --version 2>/dev/null | slurp)\n",
                "var cached-stamp = (try { cat $cache^.version } catch { echo \"\" })\n",
                "\n",
                "if (not-eq $stamp $cached-stamp) {\n",
                "    mkdir -p (path-dir $cache)\n",
                "    pup completions elvish > $cache\n",
                "    echo $stamp > $cache^.version\n",
                "}\n",
                "eval (cat $cache)\n",
            );
            let msg = format!(
                "Add the following to your ~/.elvish/rc.elv:\n  use \"{}/pup\"",
                dir.display()
            );
            Ok((loader_path, content.to_string(), msg))
        }

        _ => anyhow::bail!("unsupported shell: {shell}"),
    }
}

fn bash_completion_dir() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|h| {
            h.join(".local")
                .join("share")
                .join("bash-completion")
                .join("completions")
        })
        .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))
}

fn zsh_completion_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".zfunc")
}

fn fish_completion_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".config")
    });
    Ok(base.join("fish").join("completions"))
}

fn powershell_loader_path() -> Result<PathBuf> {
    let docs = dirs::document_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join("Documents")
    });
    Ok(docs.join("PowerShell").join("pup_completions_loader.ps1"))
}

fn elvish_completion_dir() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|h| h.join(".elvish").join("completions"))
        .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))
}
