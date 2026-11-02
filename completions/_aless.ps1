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
        { $_ -cin '--grammar', '--grammar-expr', '--max-size', '--timeout', '--depth', '--indent', '--scrolloff' } { return }
    }
    if ($null -ne $values) {
        $values | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object {
            [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $_)
        }
        return
    }
    if (-not $wordToComplete.StartsWith('-')) { return }
    @(
        [System.Management.Automation.CompletionResult]::new('--kind', '--kind', [System.Management.Automation.CompletionResultType]::ParameterName, 'parse every input as FORMAT')
        [System.Management.Automation.CompletionResult]::new('--format', '--format', [System.Management.Automation.CompletionResultType]::ParameterName, 'parse every input as FORMAT')
        [System.Management.Automation.CompletionResult]::new('-k', '-k', [System.Management.Automation.CompletionResultType]::ParameterName, 'parse every input as FORMAT')
        [System.Management.Automation.CompletionResult]::new('--grammar', '--grammar', [System.Management.Automation.CompletionResultType]::ParameterName, 'a format of your own, from an ABNF grammar')
        [System.Management.Automation.CompletionResult]::new('--grammar-expr', '--grammar-expr', [System.Management.Automation.CompletionResultType]::ParameterName, 'the same, the grammar on the command line')
        [System.Management.Automation.CompletionResult]::new('--max-size', '--max-size', [System.Management.Automation.CompletionResultType]::ParameterName, 'refuse an input over SIZE (default 64M)')
        [System.Management.Automation.CompletionResult]::new('--timeout', '--timeout', [System.Management.Automation.CompletionResultType]::ParameterName, 'stop a parse past SECONDS (default none)')
        [System.Management.Automation.CompletionResult]::new('--no-watch', '--no-watch', [System.Management.Automation.CompletionResultType]::ParameterName, 'do not reload files when they change')
        [System.Management.Automation.CompletionResult]::new('--watch', '--watch', [System.Management.Automation.CompletionResultType]::ParameterName, 'reload files when they change (the default)')
        [System.Management.Automation.CompletionResult]::new('--mode', '--mode', [System.Management.Automation.CompletionResultType]::ParameterName, 'start in data (default) or line mode')
        [System.Management.Automation.CompletionResult]::new('-m', '-m', [System.Management.Automation.CompletionResultType]::ParameterName, 'start in data (default) or line mode')
        [System.Management.Automation.CompletionResult]::new('--depth', '--depth', [System.Management.Automation.CompletionResultType]::ParameterName, 'fold containers deeper than N levels')
        [System.Management.Automation.CompletionResult]::new('--indent', '--indent', [System.Management.Automation.CompletionResultType]::ParameterName, 'indentation per level (default 2)')
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
        [System.Management.Automation.CompletionResult]::new('--help', '--help', [System.Management.Automation.CompletionResultType]::ParameterName, 'the options (-h), or the whole reference')
        [System.Management.Automation.CompletionResult]::new('-h', '-h', [System.Management.Automation.CompletionResultType]::ParameterName, 'the options (-h), or the whole reference')
        [System.Management.Automation.CompletionResult]::new('--version', '--version', [System.Management.Automation.CompletionResultType]::ParameterName, 'the version')
        [System.Management.Automation.CompletionResult]::new('-V', '-V', [System.Management.Automation.CompletionResultType]::ParameterName, 'the version')
        [System.Management.Automation.CompletionResult]::new('--generate', '--generate', [System.Management.Automation.CompletionResultType]::ParameterName, 'a man page or a shell''s completions')
    ) | Where-Object { $_.CompletionText -clike "$wordToComplete*" }
}
