# PowerShell completion for aless, written by
# `aless --generate complete-powershell`. Add it to $PROFILE, or
# dot-source it from there.
Register-ArgumentCompleter -Native -CommandName 'aless' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $words = @($commandAst.CommandElements |
        Where-Object { $_.Extent.StartOffset -lt $cursorPosition } |
        ForEach-Object { $_.ToString() })
    $prev = if ($wordToComplete -eq '') { $words[-1] } else { $words[-2] }
    $values = $null
    switch -CaseSensitive ($prev) {
        { $_ -cin '--render' } { $values = @('csv', 'ini', 'json', 'json5', 'jsonc', 'jsonic', 'jsonl', 'markdown', 'toml', 'xml', 'yaml', 'zon') }
        { $_ -cin '--kind', '--format', '-k' } { $values = @('json', 'jsonl', 'jsonic', 'jsonc', 'json5', 'yaml', 'toml', 'ini', 'csv', 'tsv', 'xml', 'zon', 'markdown', 'feed', 'text') }
        { $_ -cin '--mode', '-m' } { $values = @('data', 'line') }
        { $_ -cin '--panes' } { $values = @('out', 'program', 'out,program') }
        { $_ -cin '--generate' } { $values = @('man', 'complete-bash', 'complete-zsh', 'complete-fish', 'complete-powershell', 'skill') }
        { $_ -cin '--find', '--alchemy', '--alchemy-expr', '--path', '--at', '--limit', '--max-output', '--grammar', '--grammar-expr', '--depth', '--indent', '--max-size', '--timeout', '--scrolloff' } { return }
    }
    if ($null -ne $values) {
        $values | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object {
            [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $_)
        }
        return
    }
    if (-not $wordToComplete.StartsWith('-')) { return }
    @(
        [System.Management.Automation.CompletionResult]::new('--json', '--json', [System.Management.Automation.CompletionResultType]::ParameterName, 'the document as JSON (the default)')
        [System.Management.Automation.CompletionResult]::new('--paths', '--paths', [System.Management.Automation.CompletionResultType]::ParameterName, 'an entry for the start and each node below')
        [System.Management.Automation.CompletionResult]::new('--find', '--find', [System.Management.Automation.CompletionResultType]::ParameterName, 'the entries whose "key": value text matches')
        [System.Management.Automation.CompletionResult]::new('--where', '--where', [System.Management.Automation.CompletionResultType]::ParameterName, 'the start''s entry: its path and position')
        [System.Management.Automation.CompletionResult]::new('--check', '--check', [System.Management.Automation.CompletionResultType]::ParameterName, 'parse each FILE and report on each')
        [System.Management.Automation.CompletionResult]::new('--render', '--render', [System.Management.Automation.CompletionResultType]::ParameterName, 'the value written as FORMAT, streamed')
        [System.Management.Automation.CompletionResult]::new('--alchemy', '--alchemy', [System.Management.Automation.CompletionResultType]::ParameterName, 'run the alchemy program in FILE')
        [System.Management.Automation.CompletionResult]::new('--alchemy-expr', '--alchemy-expr', [System.Management.Automation.CompletionResultType]::ParameterName, 'the same, the program on the command line')
        [System.Management.Automation.CompletionResult]::new('--explain', '--explain', [System.Management.Automation.CompletionResultType]::ParameterName, 'the program''s plan as JSON, and no run')
        [System.Management.Automation.CompletionResult]::new('--path', '--path', [System.Management.Automation.CompletionResultType]::ParameterName, 'start at PATH, in jq syntax')
        [System.Management.Automation.CompletionResult]::new('--at', '--at', [System.Management.Automation.CompletionResultType]::ParameterName, 'start at the node at a source position')
        [System.Management.Automation.CompletionResult]::new('--limit', '--limit', [System.Management.Automation.CompletionResultType]::ParameterName, 'at most N entries (default 200; 0 for all)')
        [System.Management.Automation.CompletionResult]::new('--compact', '--compact', [System.Management.Automation.CompletionResultType]::ParameterName, 'JSON on one line, an error''s too')
        [System.Management.Automation.CompletionResult]::new('--max-output', '--max-output', [System.Management.Automation.CompletionResultType]::ParameterName, 'cap a program''s output (default 1G)')
        [System.Management.Automation.CompletionResult]::new('--kind', '--kind', [System.Management.Automation.CompletionResultType]::ParameterName, 'parse every input as FORMAT')
        [System.Management.Automation.CompletionResult]::new('--format', '--format', [System.Management.Automation.CompletionResultType]::ParameterName, 'parse every input as FORMAT')
        [System.Management.Automation.CompletionResult]::new('-k', '-k', [System.Management.Automation.CompletionResultType]::ParameterName, 'parse every input as FORMAT')
        [System.Management.Automation.CompletionResult]::new('--grammar', '--grammar', [System.Management.Automation.CompletionResultType]::ParameterName, 'a format of your own, from an ABNF grammar')
        [System.Management.Automation.CompletionResult]::new('--grammar-expr', '--grammar-expr', [System.Management.Automation.CompletionResultType]::ParameterName, 'the same, the grammar on the command line')
        [System.Management.Automation.CompletionResult]::new('--depth', '--depth', [System.Management.Automation.CompletionResultType]::ParameterName, 'list N levels down; the viewer folds deeper')
        [System.Management.Automation.CompletionResult]::new('--indent', '--indent', [System.Management.Automation.CompletionResultType]::ParameterName, 'indentation per level (default 2)')
        [System.Management.Automation.CompletionResult]::new('--max-size', '--max-size', [System.Management.Automation.CompletionResultType]::ParameterName, 'refuse an input over SIZE (default 64M)')
        [System.Management.Automation.CompletionResult]::new('--timeout', '--timeout', [System.Management.Automation.CompletionResultType]::ParameterName, 'stop a parse past SECONDS (default none)')
        [System.Management.Automation.CompletionResult]::new('--no-watch', '--no-watch', [System.Management.Automation.CompletionResultType]::ParameterName, 'do not reload files when they change')
        [System.Management.Automation.CompletionResult]::new('--watch', '--watch', [System.Management.Automation.CompletionResultType]::ParameterName, 'reload files when they change (the default)')
        [System.Management.Automation.CompletionResult]::new('--mode', '--mode', [System.Management.Automation.CompletionResultType]::ParameterName, 'start in data (default) or line mode')
        [System.Management.Automation.CompletionResult]::new('-m', '-m', [System.Management.Automation.CompletionResultType]::ParameterName, 'start in data (default) or line mode')
        [System.Management.Automation.CompletionResult]::new('--line-numbers', '--line-numbers', [System.Management.Automation.CompletionResultType]::ParameterName, 'show absolute line numbers')
        [System.Management.Automation.CompletionResult]::new('-n', '-n', [System.Management.Automation.CompletionResultType]::ParameterName, 'show absolute line numbers')
        [System.Management.Automation.CompletionResult]::new('--no-line-numbers', '--no-line-numbers', [System.Management.Automation.CompletionResultType]::ParameterName, 'hide absolute line numbers')
        [System.Management.Automation.CompletionResult]::new('-N', '-N', [System.Management.Automation.CompletionResultType]::ParameterName, 'hide absolute line numbers')
        [System.Management.Automation.CompletionResult]::new('--relative-line-numbers', '--relative-line-numbers', [System.Management.Automation.CompletionResultType]::ParameterName, 'show relative line numbers')
        [System.Management.Automation.CompletionResult]::new('-r', '-r', [System.Management.Automation.CompletionResultType]::ParameterName, 'show relative line numbers')
        [System.Management.Automation.CompletionResult]::new('--no-relative-line-numbers', '--no-relative-line-numbers', [System.Management.Automation.CompletionResultType]::ParameterName, 'hide relative line numbers')
        [System.Management.Automation.CompletionResult]::new('-R', '-R', [System.Management.Automation.CompletionResultType]::ParameterName, 'hide relative line numbers')
        [System.Management.Automation.CompletionResult]::new('--scrolloff', '--scrolloff', [System.Management.Automation.CompletionResultType]::ParameterName, 'rows kept around the focus (default 3)')
        [System.Management.Automation.CompletionResult]::new('--hidden', '--hidden', [System.Management.Automation.CompletionResultType]::ParameterName, 'show dot-files in the explorer')
        [System.Management.Automation.CompletionResult]::new('--ascii', '--ascii', [System.Management.Automation.CompletionResultType]::ParameterName, 'draw fold markers in ASCII')
        [System.Management.Automation.CompletionResult]::new('--no-color', '--no-color', [System.Management.Automation.CompletionResultType]::ParameterName, 'no colours (as NO_COLOR does)')
        [System.Management.Automation.CompletionResult]::new('--no-colour', '--no-colour', [System.Management.Automation.CompletionResultType]::ParameterName, 'no colours (as NO_COLOR does)')
        [System.Management.Automation.CompletionResult]::new('--no-mouse', '--no-mouse', [System.Management.Automation.CompletionResultType]::ParameterName, 'do not capture the mouse')
        [System.Management.Automation.CompletionResult]::new('--panes', '--panes', [System.Management.Automation.CompletionResultType]::ParameterName, 'open the output and program panes')
        [System.Management.Automation.CompletionResult]::new('--stacked', '--stacked', [System.Management.Automation.CompletionResultType]::ParameterName, 'stack the panes')
        [System.Management.Automation.CompletionResult]::new('--help', '--help', [System.Management.Automation.CompletionResultType]::ParameterName, 'the options (-h), or the whole reference')
        [System.Management.Automation.CompletionResult]::new('-h', '-h', [System.Management.Automation.CompletionResultType]::ParameterName, 'the options (-h), or the whole reference')
        [System.Management.Automation.CompletionResult]::new('--version', '--version', [System.Management.Automation.CompletionResultType]::ParameterName, 'the version')
        [System.Management.Automation.CompletionResult]::new('-V', '-V', [System.Management.Automation.CompletionResultType]::ParameterName, 'the version')
        [System.Management.Automation.CompletionResult]::new('--generate', '--generate', [System.Management.Automation.CompletionResultType]::ParameterName, 'a man page, shell completions or the skill')
    ) | Where-Object { $_.CompletionText -clike "$wordToComplete*" }
}
