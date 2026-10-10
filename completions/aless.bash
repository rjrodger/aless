# bash completion for aless, written by `aless --generate complete-bash`.
# Source it, or install it as bash-completion/completions/aless.
_aless() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    local prev="${COMP_WORDS[COMP_CWORD-1]}"
    COMPREPLY=()
    case "$prev" in
        --render)
            COMPREPLY=($(compgen -W "csv ini json json5 jsonc jsonic jsonl markdown toml xml yaml zon" -- "$cur"))
            return 0
            ;;
        --kind|--format|-k)
            COMPREPLY=($(compgen -W "json jsonl jsonic jsonc json5 yaml toml ini csv tsv xml zon markdown feed text" -- "$cur"))
            return 0
            ;;
        --mode|-m)
            COMPREPLY=($(compgen -W "data line" -- "$cur"))
            return 0
            ;;
        --panes)
            COMPREPLY=($(compgen -W "out program out,program" -- "$cur"))
            return 0
            ;;
        --generate)
            COMPREPLY=($(compgen -W "man complete-bash complete-zsh complete-fish complete-powershell skill" -- "$cur"))
            return 0
            ;;
        --find|--alchemy|--alchemy-expr|--path|--at|--limit|--max-output|--grammar|--grammar-expr|--depth|--indent|--key|--max-size|--timeout|--scrolloff)
            return 0
            ;;
    esac
    if [[ "$cur" == -* ]]; then
        COMPREPLY=($(compgen -W "--json --paths --find --where --check --render --alchemy --alchemy-expr --explain --path --at --limit --compact --max-output --kind --format -k --grammar --grammar-expr --depth --indent --key --max-size --timeout --no-watch --watch --mode -m --line-numbers -n --no-line-numbers -N --relative-line-numbers -r --no-relative-line-numbers -R --scrolloff --hidden --ascii --no-color --no-colour --no-mouse --panes --stacked --help -h --version -V --generate" -- "$cur"))
    fi
    return 0
}
complete -o default -F _aless aless
