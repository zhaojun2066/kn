# Install AI Profile Manager (Windows / PowerShell)
# Everything lives under ~/.claude-profiles/
#
#   ~/.claude-profiles/
#   ├── bin/profile        ← CLI
#   ├── lib/config.py      ← shared module
#   ├── shell-rc.ps1       ← PowerShell wrapper
#   ├── config.yaml        ← user data
#   └── .config.lock       ← file lock

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$InstallDir = Join-Path $env:USERPROFILE ".claude-profiles"
$MarkerStart = "# >>> AI Profile Manager >>>"
$MarkerEnd = "# <<< AI Profile Manager <<<"

Write-Host "==> Installing AI Profile Manager -> $InstallDir"

# Create directory structure
New-Item -ItemType Directory -Force -Path (Join-Path $InstallDir "bin") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $InstallDir "lib") | Out-Null

# Install profile CLI
Copy-Item (Join-Path $ScriptDir "bin\profile") (Join-Path $InstallDir "bin\profile") -Force
Write-Host "  ✓ bin\profile"

# Install shared config module
Copy-Item (Join-Path $ScriptDir "lib\config.py") (Join-Path $InstallDir "lib\config.py") -Force
Write-Host "  ✓ lib\config.py"

# Install PowerShell wrapper
Copy-Item (Join-Path $ScriptDir "shell\ai-profile.ps1") (Join-Path $InstallDir "shell-rc.ps1") -Force -ErrorAction SilentlyContinue
if (Test-Path (Join-Path $InstallDir "shell-rc.ps1")) {
    Write-Host "  ✓ shell-rc.ps1"
} else {
    Write-Host "  - shell-rc.ps1 (not found, skipped — PowerShell wrapper not yet available)"
}

# Install config template (only if not exists)
$ConfigPath = Join-Path $InstallDir "config.yaml"
if (-not (Test-Path $ConfigPath)) {
    Copy-Item (Join-Path $ScriptDir "templates\config.yaml") $ConfigPath -Force
    Write-Host "  ✓ config.yaml (new)"
} else {
    Write-Host "  - config.yaml (already exists, skipped)"
}

# ── Auto-activate in PowerShell profile ─────────────────────────

function Configure-PSProfile {
    param([string]$ProfilePath, [string]$ProfileName)

    # Create directory if needed
    $ProfileDir = Split-Path -Parent $ProfilePath
    if ($ProfileDir -and -not (Test-Path $ProfileDir)) {
        New-Item -ItemType Directory -Force -Path $ProfileDir | Out-Null
    }

    $existing = if (Test-Path $ProfilePath) { Get-Content $ProfilePath -Raw } else { "" }

    # Remove old marker block if exists
    if ($existing -match [regex]::Escape($MarkerStart)) {
        $existing = $existing -replace "(?ms)$([regex]::Escape($MarkerStart)).*$([regex]::Escape($MarkerEnd))", ""
        $existing = $existing.TrimEnd()
    }

    # Add PATH if not already present
    $block = @"

$MarkerStart
`$env:PATH = "$InstallDir\bin;$env:PATH"
. "$InstallDir\shell-rc.ps1"
$MarkerEnd
"@

    $newContent = if ($existing) { "$existing`r`n$block`r`n" } else { "$block`r`n" }
    Set-Content -Path $ProfilePath -Value $newContent -Encoding UTF8
    Write-Host "  ✓ $ProfileName ($ProfilePath)"
    return $true
}

$configured = $false

# Resolve Documents folder — handles redirection (e.g. OneDrive)
$docsDir = [Environment]::GetFolderPath("MyDocuments")
if (-not $docsDir) { $docsDir = Join-Path $env:USERPROFILE "Documents" }

# PowerShell 7 profile
$ps7 = Join-Path $docsDir "PowerShell\Microsoft.PowerShell_profile.ps1"
# PowerShell 5.1 profile
$ps5 = Join-Path $docsDir "WindowsPowerShell\Microsoft.PowerShell_profile.ps1"

# Prefer $PROFILE for the running PowerShell version
$currentProfile = if ($PROFILE) { $PROFILE } elseif ($PSVersionTable.PSVersion.Major -ge 7) { $ps7 } else { $ps5 }

if ($currentProfile) {
    $configured = Configure-PSProfile $currentProfile "PowerShell $($PSVersionTable.PSVersion.Major)"
}

# Also configure both if we're not sure
if (-not $configured) {
    Configure-PSProfile $ps7 "PowerShell 7"
    Configure-PSProfile $ps5 "PowerShell 5.1"
}

Write-Host ""
Write-Host "==> Done!"
Write-Host ""
Write-Host "    Run this to activate now (or restart your terminal):"
Write-Host "      . `$PROFILE"
Write-Host ""
Write-Host "    Then try:"
Write-Host "      profile list            # See all profiles"
Write-Host "      ai claude deepseek      # Launch Claude Code with deepseek profile"
