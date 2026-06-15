# kn — PowerShell Wrapper
# Usage: ai <tool> <profile>
# Sourced by PTY on startup and/or via PowerShell profile.

$script:_home = if ($env:HOME) { $env:HOME } else { $env:USERPROFILE }
# Prefer ~/.kn (new), fall back to ~/.claude-profiles (legacy)
if ($env:KN_HOME) {
    $script:_kn_config_dir = $env:KN_HOME
} elseif (Test-Path (Join-Path $script:_home ".kn")) {
    $script:_kn_config_dir = Join-Path $script:_home ".kn"
} elseif (Test-Path (Join-Path $script:_home ".claude-profiles")) {
    $script:_kn_config_dir = Join-Path $script:_home ".claude-profiles"
} else {
    $script:_kn_config_dir = Join-Path $script:_home ".kn"
}

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

function _Get-DefaultProfile {
    $cfg = Join-Path $script:_kn_config_dir "config.yaml"
    if (-not (Test-Path $cfg)) { return "" }
    $match = Get-Content $cfg | Select-String '^default:\s*"?(.+?)"?\s*$'
    if ($match) { return $match.Matches.Groups[1].Value }
    return ""
}

function _Find-ProjectProfile {
    $dir = Get-Location
    while ($dir.Path -ne $dir.Root) {
        $aiProfileFile = Join-Path $dir ".ai-profile"
        if (Test-Path $aiProfileFile) {
            $line = Get-Content $aiProfileFile | Where-Object {
                $trimmed = $_.Trim()
                $trimmed -and -not $trimmed.StartsWith("#")
            } | Select-Object -First 1
            $projName = ""
            if ($line) {
                $projName = $line.Trim() -replace '^default_profile:\s*', ''
                $projName = $projName.Trim().Trim('"').Trim("'")
            }
            if ($projName) {
                $envs = _profile_env $projName
                if ($envs -and $envs.Count -gt 0) {
                    $env:KN_PROJECT_DIR = $dir.Path
                    $env:KN_PROFILE_SOURCE = "project"
                    return $projName
                }
            }
        }
        $dir = $dir.Parent
    }
    return $null
}

function _Toml-String {
    param([string]$value)
    $escaped = $value.Replace('\', '\\').Replace('"', '\"')
    return '"' + $escaped + '"'
}

function _Launch-WithProfile {
    param([string]$tool, [string]$profileName, [string[]]$toolArgs)

    $envs = _profile_env $profileName
    if (-not $envs -or $envs.Count -eq 0) {
        Write-Host "Profile '$profileName' not found or has no env vars."
        & $tool @toolArgs
        return
    }

    Write-Host "-> Using profile: $profileName"
    $env:KN_PROFILE = $profileName
    $env:KN_CLI_TOOL = $tool
    $env:KN_WORKING_DIR = (Get-Location).Path

    # Auto-register current directory as a project (before the tool starts)
    $projDir = (Get-Location).Path
    $projFile = Join-Path $script:_kn_config_dir "projects.json"
    try {
        $projs = @()
        if (Test-Path $projFile) {
            $projs = Get-Content $projFile -Raw | ConvertFrom-Json
            if ($projs -isnot [array]) { $projs = @() }
        }
        $exists = $projs | Where-Object { $_.path -eq $projDir }
        if (-not $exists) {
            $projs += @{ name = (Split-Path $projDir -Leaf); path = $projDir }
            $parent = Split-Path $projFile -Parent
            if (-not (Test-Path $parent)) { New-Item -ItemType Directory -Force $parent | Out-Null }
            $projs | ConvertTo-Json -Depth 3 | Set-Content $projFile
        }
    } catch {}

    switch ($tool) {
        'claude' {
            # Claude Code v2.0.1+ bug: settings.json env overrides shell env.
            # Generate temp settings file via --settings (CLI flag > settings.json).
            $tmpSettings = New-TemporaryFile
            @{ env = $envs } | ConvertTo-Json -Compress | Set-Content $tmpSettings
            foreach ($key in $envs.Keys) {
                Set-Item -Path "env:$key" -Value $envs[$key]
            }
            & $tool --settings $tmpSettings @toolArgs
            Remove-Item $tmpSettings -Force -ErrorAction SilentlyContinue
        }
        'codex' {
            # Codex ignores OPENAI_API_KEY env var; reads only ~/.codex/auth.json.
            # Write the profile's key to auth.json, pass base_url/model via -c.
            $apiKey = $envs['OPENAI_API_KEY']
            $baseUrl = $envs['OPENAI_BASE_URL']
            $model = $envs['OPENAI_MODEL']
            $authFile = "$env:USERPROFILE\.codex\auth.json"
            $extraArgs = @()
            if ($model) { $extraArgs += '-c', "model=$(_Toml-String $model)" }
            if ($baseUrl) { $extraArgs += '-c', "model_providers.custom.base_url=$(_Toml-String $baseUrl)" }
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
                    & $tool @extraArgs @toolArgs
                } finally {
                    if ($oldAuth) { Set-Content $authFile $oldAuth }
                }
            } else {
                foreach ($key in $envs.Keys) {
                    Set-Item -Path "env:$key" -Value $envs[$key]
                }
                & $tool @extraArgs @toolArgs
            }
        }
        default {
            foreach ($key in $envs.Keys) {
                Set-Item -Path "env:$key" -Value $envs[$key]
            }
            & $tool @toolArgs
        }
    }
}

function _Try-Launch-Explicit {
    param([string]$tool, [string[]]$rest)
    if ($rest.Count -gt 0) {
        $envs = _profile_env $rest[0]
        if ($envs -and $envs.Count -gt 0) {
            _Launch-WithProfile $tool $rest[0] @($rest | Select-Object -Skip 1)
            return $true
        }
    }
    return $false
}

function _Try-Launch-Fallback {
    param([string]$tool, [string[]]$rest)
    # Project-level .ai-profile
    $projProfile = _Find-ProjectProfile
    if ($projProfile) {
        _Launch-WithProfile $tool $projProfile $rest
        return $true
    }
    # Default profile
    $defaultProfile = _Get-DefaultProfile
    if ($defaultProfile) {
        _Launch-WithProfile $tool $defaultProfile $rest
        return $true
    }
    return $false
}

# ── Tips (model selection recommendations + personalized stats) ──
function _ai_tips {
    Write-Host "AI Model Tips:"
    Write-Host "  编程开发   → claude-sonnet-4-6 / deepseek-v3"
    Write-Host "  复杂推理   → claude-opus-4-8 / deepseek-reasoner"
    Write-Host "  快速修改   → claude-haiku-4-5"
    Write-Host "  中文场景   → deepseek-chat / deepseek-v4-pro"
    Write-Host ""

    # Try to compute personalized top 3 from PowerShell history
    $histfile = (Get-PSReadLineOption).HistorySavePath
    if (-not $histfile) {
        $histfile = Join-Path (Split-Path $profile) "ConsoleHost_history.txt"
    }
    if ($histfile -and (Test-Path $histfile)) {
        try {
            $counts = @{}
            Get-Content $histfile -Tail 5000 | ForEach-Object {
                $line = $_.Trim()
                if ($line -match '^ai\s+(claude|codex|qoderclicn)\s+(\S+)') {
                    $pn = $Matches[2]
                    if (-not $counts[$pn]) { $counts[$pn] = 0 }
                    $counts[$pn]++
                }
            }
            $top = $counts.GetEnumerator() | Sort-Object Value -Descending | Select-Object -First 3
            if ($top) {
                Write-Host "  你最常用:"
                foreach ($p in $top) {
                    Write-Host "    $($p.Name) ($($p.Value) 次)"
                }
            } else {
                Write-Host "  你最常用:   尚未使用 profile，试试 ai claude <profile> 吧"
            }
        } catch {
            Write-Host "  你最常用:   无法读取历史文件"
        }
    } else {
        Write-Host "  你最常用:   无法读取历史文件，试试 ai claude <profile> 吧"
    }
}

# ── Profile switch (set default) ──
function _profile_switch {
    param([string]$name)
    if (-not $name) {
        Write-Host "Usage: ai profile switch <name>"
        return
    }
    $cfg = Join-Path $script:_kn_config_dir "config.yaml"
    if (-not (Test-Path $cfg)) {
        Write-Host "Config not found: $cfg"
        return
    }
    # Check profile exists
    $found = $false
    Get-Content $cfg | ForEach-Object {
        if ($_ -match "^  ${name}:") { $found = $true }
    }
    if (-not $found) {
        Write-Host "Profile not found: $name"
        return
    }
    # Update default line
    $content = Get-Content $cfg -Raw
    $newContent = $content -replace '(?m)^default:.*$', "default: $name"
    Set-Content $cfg $newContent -NoNewline
    Write-Host "Default profile set to: $name"
}

function ai {
    $cmd = $args[0]

    if (-not $cmd) {
        Write-Host "kn — AI CLI Workspace Manager"
        Write-Host ""
        Write-Host "  Run with profile:"
        Write-Host "    ai claude <profile>       Run Claude Code with profile env"
        Write-Host "    ai codex <profile>        Run Codex CLI with profile env"
        Write-Host "    ai qoderclicn <profile>   Run Qoder with profile env"
        Write-Host ""
        Write-Host "  Manage profiles:"
        Write-Host "    ai profile list           List all profiles"
        Write-Host "    ai profile env <name>     Show env vars for a profile"
        Write-Host "    ai profile switch <name>  Set default profile"
        Write-Host "    ai tips                   Show model recommendations"
        return
    }

    switch ($cmd) {
        'claude' {
            $tool = 'claude'
            $rest = @($args | Select-Object -Skip 1)
            if (_Try-Launch-Explicit $tool $rest) { return }
            if (_Try-Launch-Fallback $tool $rest) { return }
            & $tool @rest
        }
        'codex' {
            $tool = 'codex'
            $rest = @($args | Select-Object -Skip 1)
            if (_Try-Launch-Explicit $tool $rest) { return }
            if (_Try-Launch-Fallback $tool $rest) { return }
            & $tool @rest
        }
        'qoderclicn' {
            $tool = 'qoderclicn'
            $rest = @($args | Select-Object -Skip 1)
            if (_Try-Launch-Explicit $tool $rest) { return }
            if (_Try-Launch-Fallback $tool $rest) { return }
            & $tool @rest
        }
        'tips' {
            _ai_tips
        }
        'profile' {
            $subcmd = $args[1]
            switch ($subcmd) {
                'list' { _profile_list }
                'env' { _profile_show $args[2] }
                'switch' { _profile_switch $args[2] }
                default {
                    Write-Host "Usage: ai profile {list|env <name>|switch <name>}"
                }
            }
        }
        { $_ -in @('-h','--help','help') } {
            Write-Host "kn — AI CLI Workspace Manager"
            Write-Host "  ai claude <profile>       Run Claude Code with profile"
            Write-Host "  ai codex <profile>        Run Codex CLI with profile"
            Write-Host "  ai qoderclicn <profile>   Run Qoder with profile"
            Write-Host "  ai profile list           List all profiles"
            Write-Host "  ai profile env <name>     Show env vars for profile"
            Write-Host "  ai profile switch <name>  Set default profile"
            Write-Host "  ai tips                   Show model recommendations"
        }
        default {
            Write-Host "Unknown command: $cmd"
            Write-Host "Supported: claude, codex, qoderclicn, profile, tips"
        }
    }
}
