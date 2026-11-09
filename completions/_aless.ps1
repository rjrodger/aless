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
        { $_ -cin '--kind', '--format', '-k' } { $values = @('json', 'jsonl', 'jsonic', 'jsonc', 'json5', 'yaml', 'toml', 'ini', 'csv', 'tsv', 'xml', 'zon', 'markdown', 'feed', 'css', 'proto', 'pgn', 'c', 'abnf', 'ebnf', 'gbnf', 'expr', 'semver', 'text') }
        { $_ -cin '--mode', '-m' } { $values = @('data', 'line') }
        { $_ -cin '--generate' } { $values = @('man', 'complete-bash', 'complete-zsh', 'complete-fish', 'complete-powershell') }
        { $_ -cin '--grammar', '--grammar-expr', '--max-size', '--timeout', '--depth', '--indent', '--scrolloff', '--print-json' } { return }
    }
    if ($null -ne $values) {
        $values | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object {
            [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $_)
        }
        return
    }
    if (-not $wordToComplete.StartsWith('-')) { return }
    @(
        [System.Management.Automation.CompletionResult]::new('--kind', '--kind', [System.Management.Automation.CompletionResultType]::ParameterName, 'read every input as FORMAT')
        [System.Management.Automation.CompletionResult]::new('--format', '--format', [System.Management.Automation.CompletionResultType]::ParameterName, 'read every input as FORMAT')
        [System.Management.Automation.CompletionResult]::new('-k', '-k', [System.Management.Automation.CompletionResultType]::ParameterName, 'read every input as FORMAT')
        [System.Management.Automation.CompletionResult]::new('--grammar', '--grammar', [System.Management.Automation.CompletionResultType]::ParameterName, 'read your own format, from an ABNF grammar')
        [System.Management.Automation.CompletionResult]::new('--grammar-expr', '--grammar-expr', [System.Management.Automation.CompletionResultType]::ParameterName, 'read your own format, from command-line ABNF')
        [System.Management.Automation.CompletionResult]::new('--max-size', '--max-size', [System.Management.Automation.CompletionResultType]::ParameterName, 'refuse an input over SIZE (default 64M)')
        [System.Management.Automation.CompletionResult]::new('--timeout', '--timeout', [System.Management.Automation.CompletionResultType]::ParameterName, 'stop a parse past SECONDS (default none)')
        [System.Management.Automation.CompletionResult]::new('--no-watch', '--no-watch', [System.Management.Automation.CompletionResultType]::ParameterName, 'do not reload a file when it changes')
        [System.Management.Automation.CompletionResult]::new('--watch', '--watch', [System.Management.Automation.CompletionResultType]::ParameterName, 'reload a file when it changes (the default)')
        [System.Management.Automation.CompletionResult]::new('--no-changes', '--no-changes', [System.Management.Automation.CompletionResultType]::ParameterName, 'do not mark what a reload changed')
        [System.Management.Automation.CompletionResult]::new('--mode', '--mode', [System.Management.Automation.CompletionResultType]::ParameterName, 'start in data (default) or line mode')
        [System.Management.Automation.CompletionResult]::new('-m', '-m', [System.Management.Automation.CompletionResultType]::ParameterName, 'start in data (default) or line mode')
        [System.Management.Automation.CompletionResult]::new('--depth', '--depth', [System.Management.Automation.CompletionResultType]::ParameterName, 'open only the first N levels')
        [System.Management.Automation.CompletionResult]::new('--indent', '--indent', [System.Management.Automation.CompletionResultType]::ParameterName, 'indent N spaces a level (default 2)')
        [System.Management.Automation.CompletionResult]::new('--line-numbers', '--line-numbers', [System.Management.Automation.CompletionResultType]::ParameterName, 'number the rows')
        [System.Management.Automation.CompletionResult]::new('-n', '-n', [System.Management.Automation.CompletionResultType]::ParameterName, 'number the rows')
        [System.Management.Automation.CompletionResult]::new('--no-line-numbers', '--no-line-numbers', [System.Management.Automation.CompletionResultType]::ParameterName, 'do not number the rows')
        [System.Management.Automation.CompletionResult]::new('-N', '-N', [System.Management.Automation.CompletionResultType]::ParameterName, 'do not number the rows')
        [System.Management.Automation.CompletionResult]::new('--relative-line-numbers', '--relative-line-numbers', [System.Management.Automation.CompletionResultType]::ParameterName, 'number rows by distance from the focus')
        [System.Management.Automation.CompletionResult]::new('-r', '-r', [System.Management.Automation.CompletionResultType]::ParameterName, 'number rows by distance from the focus')
        [System.Management.Automation.CompletionResult]::new('--no-relative-line-numbers', '--no-relative-line-numbers', [System.Management.Automation.CompletionResultType]::ParameterName, 'do not number rows by distance')
        [System.Management.Automation.CompletionResult]::new('-R', '-R', [System.Management.Automation.CompletionResultType]::ParameterName, 'do not number rows by distance')
        [System.Management.Automation.CompletionResult]::new('--scrolloff', '--scrolloff', [System.Management.Automation.CompletionResultType]::ParameterName, 'keep N rows around the focus (default 3)')
        [System.Management.Automation.CompletionResult]::new('--ascii', '--ascii', [System.Management.Automation.CompletionResultType]::ParameterName, 'draw fold markers in ASCII')
        [System.Management.Automation.CompletionResult]::new('--no-color', '--no-color', [System.Management.Automation.CompletionResultType]::ParameterName, 'draw without colour, as NO_COLOR does')
        [System.Management.Automation.CompletionResult]::new('--no-colour', '--no-colour', [System.Management.Automation.CompletionResultType]::ParameterName, 'draw without colour, as NO_COLOR does')
        [System.Management.Automation.CompletionResult]::new('--no-mouse', '--no-mouse', [System.Management.Automation.CompletionResultType]::ParameterName, 'leave the mouse to the terminal')
        [System.Management.Automation.CompletionResult]::new('--help', '--help', [System.Management.Automation.CompletionResultType]::ParameterName, 'print the options (-h) or the reference')
        [System.Management.Automation.CompletionResult]::new('-h', '-h', [System.Management.Automation.CompletionResultType]::ParameterName, 'print the options (-h) or the reference')
        [System.Management.Automation.CompletionResult]::new('--version', '--version', [System.Management.Automation.CompletionResultType]::ParameterName, 'print the version')
        [System.Management.Automation.CompletionResult]::new('-V', '-V', [System.Management.Automation.CompletionResultType]::ParameterName, 'print the version')
        [System.Management.Automation.CompletionResult]::new('--generate', '--generate', [System.Management.Automation.CompletionResultType]::ParameterName, 'print the man page or a shell''s completions')
        [System.Management.Automation.CompletionResult]::new('--print-json', '--print-json', [System.Management.Automation.CompletionResultType]::ParameterName, 'print FILE''s tree as JSON, and exit')
    ) | Where-Object { $_.CompletionText -clike "$wordToComplete*" }
}
