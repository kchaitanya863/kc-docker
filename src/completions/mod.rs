use crate::storage::boxr_home;
use anyhow::{Result, anyhow};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub enum ShellType {
    Bash,
    Zsh,
    Fish,
}

impl ShellType {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "bash" => Ok(ShellType::Bash),
            "zsh" => Ok(ShellType::Zsh),
            "fish" => Ok(ShellType::Fish),
            other => Err(anyhow!(
                "Unsupported shell '{}', expected bash, zsh, or fish",
                other
            )),
        }
    }
}

pub struct CompletionGenerator;

impl CompletionGenerator {
    pub fn generate(shell: ShellType) -> String {
        match shell {
            ShellType::Bash => Self::generate_bash(),
            ShellType::Zsh => Self::generate_zsh(),
            ShellType::Fish => Self::generate_fish(),
        }
    }

    fn generate_bash() -> String {
        r#"# Bash completion for boxr
_boxr() {
    local cur prev commands
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    commands="run pull stop start logs exec inspect build compose volume network daemon stats events save load login logout images ps rm rmi spec completion alias"

    case "${prev}" in
        volume)
            COMPREPLY=( $(compgen -W "create ls inspect rm prune" -- "${cur}") )
            return 0
            ;;
        network)
            COMPREPLY=( $(compgen -W "create ls inspect rm connect disconnect" -- "${cur}") )
            return 0
            ;;
        compose)
            COMPREPLY=( $(compgen -W "up down ps logs" -- "${cur}") )
            return 0
            ;;
        builder)
            COMPREPLY=( $(compgen -W "prune" -- "${cur}") )
            return 0
            ;;
        completion)
            COMPREPLY=( $(compgen -W "bash zsh fish" -- "${cur}") )
            return 0
            ;;
    esac

    if [[ ${cur} == -* ]] ; then
        COMPREPLY=( $(compgen -W "--help --version -i --interactive -d --detach --rm --name -e --env -p --publish -v --volume --memory --cpus --pids-limit --rootless" -- "${cur}") )
        return 0
    fi

    COMPREPLY=( $(compgen -W "${commands}" -- "${cur}") )
    return 0
}
complete -F _boxr boxr
"#
        .to_string()
    }

    fn generate_zsh() -> String {
        r#"#compdef boxr

_boxr() {
    local -a commands
    commands=(
        'run:Run a command in a new container'
        'pull:Pull an image from an OCI registry'
        'stop:Stop a running container'
        'start:Start a stopped container'
        'logs:Fetch container logs'
        'exec:Execute command in a container'
        'inspect:Return low-level information on Boxr objects'
        'build:Build an image from a Dockerfile'
        'compose:Multi-container orchestration'
        'volume:Manage volumes'
        'network:Manage networks'
        'daemon:Run background REST API daemon'
        'stats:Display live container resource statistics'
        'events:Stream container lifecycle events'
        'save:Save image to tar archive'
        'load:Load image from tar archive'
        'login:Log in to registry'
        'logout:Log out from registry'
        'images:List local images'
        'ps:List containers'
        'rm:Remove containers'
        'rmi:Remove images'
        'spec:Generate OCI runtime spec'
        'completion:Generate shell autocompletions'
        'alias:Install Docker drop-in alias'
    )

    _arguments -C \
        '1: :->command' \
        '*:: :->args'

    case $state in
        command)
            _describe -t commands 'boxr commands' commands
            ;;
    esac
}

_boxr "$@"
"#
        .to_string()
    }

    fn generate_fish() -> String {
        r#"# Fish completion for boxr
complete -c boxr -f
complete -c boxr -n "__fish_use_subcommand" -a run -d 'Run a container'
complete -c boxr -n "__fish_use_subcommand" -a pull -d 'Pull an image'
complete -c boxr -n "__fish_use_subcommand" -a stop -d 'Stop a container'
complete -c boxr -n "__fish_use_subcommand" -a start -d 'Start a container'
complete -c boxr -n "__fish_use_subcommand" -a logs -d 'Fetch logs'
complete -c boxr -n "__fish_use_subcommand" -a exec -d 'Execute in container'
complete -c boxr -n "__fish_use_subcommand" -a inspect -d 'Inspect objects'
complete -c boxr -n "__fish_use_subcommand" -a build -d 'Build image from Dockerfile'
complete -c boxr -n "__fish_use_subcommand" -a compose -d 'Compose multi-container app'
complete -c boxr -n "__fish_use_subcommand" -a volume -d 'Manage volumes'
complete -c boxr -n "__fish_use_subcommand" -a network -d 'Manage networks'
complete -c boxr -n "__fish_use_subcommand" -a daemon -d 'Start daemon'
complete -c boxr -n "__fish_use_subcommand" -a stats -d 'Live container stats'
complete -c boxr -n "__fish_use_subcommand" -a events -d 'Stream container events'
complete -c boxr -n "__fish_use_subcommand" -a save -d 'Save image tar'
complete -c boxr -n "__fish_use_subcommand" -a load -d 'Load image tar'
complete -c boxr -n "__fish_use_subcommand" -a login -d 'Login to registry'
complete -c boxr -n "__fish_use_subcommand" -a logout -d 'Logout from registry'
complete -c boxr -n "__fish_use_subcommand" -a images -d 'List images'
complete -c boxr -n "__fish_use_subcommand" -a ps -d 'List containers'
complete -c boxr -n "__fish_use_subcommand" -a rm -d 'Remove containers'
complete -c boxr -n "__fish_use_subcommand" -a rmi -d 'Remove images'
complete -c boxr -n "__fish_use_subcommand" -a spec -d 'Generate OCI spec'
complete -c boxr -n "__fish_use_subcommand" -a completion -d 'Generate shell completions'
complete -c boxr -n "__fish_use_subcommand" -a alias -d 'Docker drop-in alias'
"#
        .to_string()
    }

    /// Install Docker drop-in wrapper in ~/.boxr/bin/docker
    pub fn install_docker_wrapper() -> Result<String> {
        let home = boxr_home();
        let bin_dir = home.join("bin");
        fs::create_dir_all(&bin_dir)?;

        let current_exe = std::env::current_exe()?;
        let current_exe_str = current_exe.to_string_lossy();

        let wrapper_script = format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", current_exe_str);

        let wrapper_path = bin_dir.join("docker");
        fs::write(&wrapper_path, wrapper_script)?;

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&wrapper_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&wrapper_path, perms)?;
        }

        Ok(bin_dir.to_string_lossy().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_generation() {
        let bash = CompletionGenerator::generate(ShellType::Bash);
        assert!(bash.contains("complete -F _boxr boxr"));

        let zsh = CompletionGenerator::generate(ShellType::Zsh);
        assert!(zsh.contains("#compdef boxr"));

        let fish = CompletionGenerator::generate(ShellType::Fish);
        assert!(fish.contains("complete -c boxr"));
    }
}
