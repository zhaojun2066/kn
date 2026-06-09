# AI Profile Manager — PowerShell Wrapper
# Usage: ai <tool> <profile>
# Sourced by PTY on startup and/or via PowerShell profile.

$script:_kn_config_dir = if ($env:HOME) { "$env:HOME\.claude-profiles" } else { "$env:USERPROFILE\.claude-profiles" }

function _profile_env {
    param([string]$name)
    $cfg = Join-Path $script:_kn_config_dir "config.yaml"
    if (-not (Test-Path $cfg)) { return @{} }
    $env_vars = @{}
    $inProfile = $false; $inEnv = $false
    $escaped = [regex]::Escape($name)
    Get-Content $cfg | ForEach-Object {
        $line = $_
        if ($line -match "^  ${escaped}:") { $inProfile = $true; return }
        if ($inProfile -and $line -match "^    env:") { $inEnv = $true; return }
        if ($inEnv) {
            # Next top-level section ends this profile's env block
            if ($line -match "^  [a-z]") { $inProfile = $false; $inEnv = $false; return }
            # Parse key: value (7 leading spaces for env vars)
            if ($line -match '^      ([A-Za-z_][A-Za-z0-9_]*):\s*(.*)$') {
                $key = $Matches[1]
                $rawVal = $Matches[2]
                # Handle empty / explicit-empty values
                if ($rawVal -eq '' -or $rawVal -eq '""' -or $rawVal -eq "''") {
                    $env_vars[$key] = ""
                    return
                }
                # Strip surrounding double quotes
                if ($rawVal[0] -eq '"' -and $rawVal[-1] -eq '"') {
                    $env_vars[$key] = $rawVal.Substring(1, $rawVal.Length - 2)
                    return
                }
                # Strip surrounding single quotes
                if ($rawVal[0] -eq "'" -and $rawVal[-1] -eq "'") {
                    $env_vars[$key] = $rawVal.Substring(1, $rawVal.Length - 2)
                    return
                }
                # Unquoted value: truncate YAML inline comments (#)
                $hashIdx = $rawVal.IndexOf('#')
                if ($hashIdx -ge 0) { $rawVal = $rawVal.Substring(0, $hashIdx).TrimEnd() }
                $env_vars[$key] = $rawVal
            }
        }
    }
    return $env_vars
}

function _profile_list {
    $cfg = Join-Path $script:_kn_config_dir "config.yaml"
    if (-not (Test-Path $cfg)) { Write-Host "No config found at $cfg"; return }
    $defaultMatch = Get-Content $cfg | Select-String '^default:\s*"?(.+?)"?\s*$'
    $default = if ($defaultMatch) { $defaultMatch.Matches.Groups[1].Value } else { "" }
    Get-Content $cfg | Select-String '^  ([a-zA-Z0-9_-]+):' | ForEach-Object {
        $n = $_.Matches.Groups[1].Value
        if ($n -eq $default) { Write-Host "  $n (*)" } else { Write-Host "  $n" }
    }
}

function _profile_show {
    param([string]$name)
    $vars = _profile_env $name
    if (-not $vars -or $vars.Count -eq 0) { Write-Host "Profile '$name' not found"; return }
    Write-Host "Profile: $name"
    foreach ($key in ($vars.Keys | Sort-Object)) {
        Write-Host "  $key=$($vars[$key])"
    }
}

function ai {
    $cmd = $args[0]

    if (-not $cmd) {
        Write-Host "Usage: ai <tool> [args...]"
        Write-Host ""
        Write-Host "  Run with profile:"
        Write-Host "    ai claude <profile>       Run Claude Code with profile env"
        Write-Host "    ai codex <profile>        Run Codex CLI with profile env"
        Write-Host "    ai qoderclicn <profile>   Run Qoder with profile env"
        Write-Host ""
        Write-Host "  Manage profiles:"
        Write-Host "    ai profile list           List all profiles"
        Write-Host "    ai profile env <name>     Show env vars for a profile"
        return
    }

    switch ($cmd) {
        'claude' {
            $tool = 'claude'
            $rest = @($args | Select-Object -Skip 1)
            if ($rest.Count -gt 0) {
                $envs = _profile_env $rest[0]
                if ($envs -and $envs.Count -gt 0) {
                    $profileName = $rest[0]
                    $toolArgs = @($rest | Select-Object -Skip 1)
                    Write-Host "-> Using profile: $profileName"
                    # Claude Code v2.0.1+ bug: settings.json env overrides shell env.
                    # Generate temp settings file via --settings (CLI flag > settings.json).
                    $tmpSettings = New-TemporaryFile
                    @{ env = $envs } | ConvertTo-Json -Compress | Set-Content $tmpSettings
                    foreach ($key in $envs.Keys) {
                        Set-Item -Path "env:$key" -Value $envs[$key]
                    }
                    $env:KN_PROFILE = $profileName
                    $env:KN_CLI_TOOL = $tool
                    & $tool --settings $tmpSettings @toolArgs
                    Remove-Item $tmpSettings -Force -ErrorAction SilentlyContinue
                    return
                }
            }
            & $tool @rest
        }
        'codex' {
            $tool = 'codex'
            $rest = @($args | Select-Object -Skip 1)
            if ($rest.Count -gt 0) {
                $envs = _profile_env $rest[0]
                if ($envs -and $envs.Count -gt 0) {
                    $profileName = $rest[0]
                    $toolArgs = @($rest | Select-Object -Skip 1)
                    Write-Host "-> Using profile: $profileName"
                    # Codex ignores OPENAI_API_KEY env var; reads only ~/.codex/auth.json.
                    # Write the profile's key to auth.json, pass base_url/model via -c.
                    $apiKey = $envs['OPENAI_API_KEY']
                    $baseUrl = $envs['OPENAI_BASE_URL']
                    $model = $envs['OPENAI_MODEL']
                    $authFile = "$env:USERPROFILE\.codex\auth.json"
                    $extraArgs = @()
                    if ($model) { $extraArgs += '-c', "model=$model" }
                    if ($baseUrl) { $extraArgs += '-c', "model_providers.custom.base_url=$baseUrl" }
                    if ($apiKey) {
                        $codexDir = Split-Path $authFile -Parent
                        if (-not (Test-Path $codexDir)) { New-Item -ItemType Directory -Force $codexDir | Out-Null }
                        $oldAuth = $null
                        if (Test-Path $authFile) { $oldAuth = Get-Content $authFile -Raw }
                        @{ auth_mode = "apikey"; OPENAI_API_KEY = $apiKey } | ConvertTo-Json -Compress | Set-Content $authFile
                        try {
                            foreach ($key in $envs.Keys) {
                                Set-Item -Path "env:$key" -Value $envs[$key]
                            }
                            $env:KN_PROFILE = $profileName
                            $env:KN_CLI_TOOL = $tool
                            & $tool @extraArgs @toolArgs
                        } finally {
                            if ($oldAuth) { Set-Content $authFile $oldAuth }
                        }
                        return
                    }
                    foreach ($key in $envs.Keys) {
                        Set-Item -Path "env:$key" -Value $envs[$key]
                    }
                    $env:KN_PROFILE = $profileName
                    $env:KN_CLI_TOOL = $tool
                    & $tool @extraArgs @toolArgs
                    return
                }
            }
            & $tool @rest
        }
        'qoderclicn' {
            $tool = 'qoderclicn'
            $rest = @($args | Select-Object -Skip 1)
            if ($rest.Count -gt 0) {
                $envs = _profile_env $rest[0]
                if ($envs -and $envs.Count -gt 0) {
                    $profileName = $rest[0]
                    $toolArgs = @($rest | Select-Object -Skip 1)
                    Write-Host "-> Using profile: $profileName"
                    foreach ($key in $envs.Keys) {
                        Set-Item -Path "env:$key" -Value $envs[$key]
                    }
                    $env:KN_PROFILE = $profileName
                    $env:KN_CLI_TOOL = $tool
                    & $tool @toolArgs
                    return
                }
            }
            & $tool @rest
        }
        'profile' {
            $subcmd = $args[1]
            switch ($subcmd) {
                'list' { _profile_list }
                'env' { _profile_show $args[2] }
                default {
                    Write-Host "Usage: ai profile {list|env <name>}"
                }
            }
        }
        { $_ -in @('-h','--help','help') } {
            Write-Host "AI Profile Manager"
            Write-Host "  ai claude <profile>       Run Claude Code with profile"
            Write-Host "  ai codex <profile>        Run Codex CLI with profile"
            Write-Host "  ai qoderclicn <profile>   Run Qoder with profile"
            Write-Host "  ai profile list           List all profiles"
            Write-Host "  ai profile env <name>     Show env vars for profile"
        }
        default {
            Write-Host "Unknown command: $cmd"
            Write-Host "Supported: claude, codex, qoderclicn, profile"
        }
    }
}
