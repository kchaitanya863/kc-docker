# Bash completion for boxr
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

