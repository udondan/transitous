//! Shell completion.
//!
//! The scripts call `transitous` on every completion with the words typed so
//! far in the environment variable [`VAR`], and `transitous` answers from its
//! clap definition. So the scripts stay small and follow every change of the
//! commands. Completion never contacts Transitous.

use std::ffi::OsStr;
use std::io;

use clap::ValueEnum;
use clap::builder::{EnumValueParser, PossibleValue, TypedValueParser};
use clap_complete::env::{CompleteEnv, Shells};

/// The environment variable that asks `transitous` for completions instead
/// of running a command.
const VAR: &str = "COMPLETE";

/// How to install the scripts, for the help. Each shell loads the script
/// from `transitous` when it starts, so it follows updates.
pub(crate) const INSTALL: &str = r#"Install, so that the script follows updates of transitous:

bash:
  echo 'eval "$(transitous completion bash)"' \
    > ~/.local/share/bash-completion/completions/transitous

zsh:
  printf '%s\n' '#compdef transitous' 'eval "$(transitous completion zsh)"' \
    > "${fpath[1]}/_transitous"

fish:
  echo 'transitous completion fish | source' \
    > ~/.config/fish/completions/transitous.fish

elvish:
  echo 'eval (transitous completion elvish | slurp)' >> ~/.config/elvish/rc.elv

PowerShell:
  Add-Content $PROFILE 'transitous completion powershell | Out-String | Invoke-Expression'"#;

/// The program that the scripts complete and call, found on the `PATH`.
const BIN: &str = "transitous";

/// Loaded from `$fpath`, the zsh script runs in the function `_transitous`,
/// which only registers the completer. This also completes on the first
/// tab, also when `_transitous` loads the script with `eval` or `source`.
const ZSH_AUTOLOAD: &str =
    r#"if (( ${funcstack[(Ie)_transitous]} )); then _clap_dynamic_completer_transitous "$@"; fi"#;

/// The shells with completion scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Shell {
    Bash,
    Elvish,
    Fish,
    #[value(alias = "pwsh")]
    Powershell,
    Zsh,
}

/// Parses a shell by its name or the path of its program, like `$SHELL`:
/// `/bin/zsh`, `-zsh` of a login shell or `pwsh.exe` work as well.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ShellParser;

impl TypedValueParser for ShellParser {
    type Value = Shell;

    fn parse_ref(
        &self,
        command: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &OsStr,
    ) -> Result<Shell, clap::Error> {
        let name = value.to_str().map_or(value, |v| OsStr::new(shell_name(v)));
        EnumValueParser::<Shell>::new().parse_ref(command, arg, name)
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            Shell::value_variants()
                .iter()
                .filter_map(ValueEnum::to_possible_value),
        ))
    }
}

/// The name of a shell in its program path, without the dash of a login
/// shell and without `.exe`.
fn shell_name(value: &str) -> &str {
    let name = value.rsplit(['/', '\\']).next().unwrap_or(value);
    let name = name.trim_start_matches('-');
    name.strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".EXE"))
        .unwrap_or(name)
}

impl Shell {
    fn name(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Elvish => "elvish",
            Self::Fish => "fish",
            Self::Powershell => "powershell",
            Self::Zsh => "zsh",
        }
    }
}

/// Answers a completion request of a script and exits, if [`VAR`] is set.
/// Returns otherwise.
pub(crate) fn complete(command: fn() -> clap::Command) {
    CompleteEnv::with_factory(command)
        .var(VAR)
        .bin(BIN)
        .completer(BIN)
        .complete();
}

/// Writes the completion script for the shell.
pub(crate) fn script(shell: Shell, out: &mut impl io::Write) -> io::Result<()> {
    let shells = Shells::builtins();
    let completer = shells
        .completer(shell.name())
        .expect("clap_complete supports every shell of Shell");
    completer.write_registration(VAR, BIN, BIN, BIN, out)?;
    if let Shell::Zsh = shell {
        writeln!(out, "{ZSH_AUTOLOAD}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shells_by_their_path() {
        let command = clap::Command::new("test");
        let parse = |value: &str| ShellParser.parse_ref(&command, None, OsStr::new(value));
        for (value, shell) in [
            ("zsh", Shell::Zsh),
            ("/bin/zsh", Shell::Zsh),
            ("/opt/homebrew/bin/bash", Shell::Bash),
            ("-zsh", Shell::Zsh),
            ("/usr/local/bin/fish", Shell::Fish),
            ("pwsh", Shell::Powershell),
            (
                "C:\\Program Files\\PowerShell\\7\\pwsh.exe",
                Shell::Powershell,
            ),
            ("pwsh.EXE", Shell::Powershell),
            ("elvish", Shell::Elvish),
        ] {
            assert_eq!(parse(value).unwrap(), shell, "{value}");
        }
        assert!(parse("/bin/sh").is_err());
        assert_eq!(ShellParser.possible_values().unwrap().count(), 5);
    }

    #[test]
    fn writes_the_scripts() {
        for shell in Shell::value_variants() {
            let mut out = Vec::new();
            script(*shell, &mut out).unwrap();
            let text = String::from_utf8(out).unwrap();
            assert!(text.contains("transitous"), "{shell:?}");
            assert_eq!(text.contains("funcstack"), *shell == Shell::Zsh);
        }
    }
}
