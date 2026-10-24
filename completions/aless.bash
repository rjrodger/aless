# bash completion for aless, written by `aless --generate complete-bash`.
# Source it, or install it as bash-completion/completions/aless.
_aless() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    local prev="${COMP_WORDS[COMP_CWORD-1]}"
    COMPREPLY=()
    case "$prev" in
        --kind|--format|-k)
            COMPREPLY=($(compgen -W "json jsonl jsonic jsonc json5 yaml toml ini csv tsv xml zon markdown feed css proto pgn c abnf ebnf gbnf expr semver text" -- "$cur"))
            return 0
            ;;
        --mode|-m)
            COMPREPLY=($(compgen -W "data line" -- "$cur"))
            return 0
            ;;
        --generate)
            COMPREPLY=($(compgen -W "man complete-bash complete-zsh complete-fish complete-powershell skill" -- "$cur"))
            return 0
            ;;
        --find|--path|--at|--limit|--grammar|--grammar-expr|--depth|--indent|--max-size|--timeout|--scrolloff)
            return 0
            ;;
    esac
    if [[ "$cur" == -* ]]; then
        COMPREPLY=($(compgen -W "--json --paths --find --where --check --path --at --limit --compact --kind --format -k --grammar --grammar-expr --depth --indent --max-size --timeout --no-watch --watch --mode -m --line-numbers -n --no-line-numbers -N --relative-line-numbers -r --no-relative-line-numbers -R --scrolloff --hidden --ascii --no-color --no-colour --no-mouse --help -h --version -V --generate" -- "$cur"))
    fi
    return 0
}
complete -o default -F _aless aless
