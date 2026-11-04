# Makes the Firefox copy of the extension:
#
#   .\installer\make-firefox.ps1 -Out firefox
#
# Chrome and Edge load the extension folder as it is. Firefox needs another
# manifest: background scripts instead of a service worker, its own extension
# ID, and no Chrome key; Chrome warns about the first two in Manifest V3, so
# they cannot share one file. The copy is the extension folder without its
# tests, with manifest.json changed by extension\firefox.json: each key there
# replaces the manifest's, and null removes it. extension\test\harness.js
# does the same for the tests, and installer/make-firefox.js for macOS and
# Linux.
#
# The copy is made, not kept in git, so one made before the extension
# changed silently lacks the change (#149). This tells whether a copy is
# still what the extension makes, listing what differs, and exits with 1 if
# it is not:
#
#   .\installer\make-firefox.ps1 -Check firefox
#
# Every file must be byte for byte the same and none missing or extra;
# manifest.json is compared as JSON, not as text. CI checks that this and
# make-firefox.js make the same copy.

param(
    [string]$Out,
    [string]$Check
)

$ErrorActionPreference = "Stop"
$source = Join-Path (Split-Path $PSScriptRoot) "extension"
$skipped = "test", "firefox.json", "manifest.json"

if ([bool]$Out -eq [bool]$Check) {
    Write-Host "usage: .\installer\make-firefox.ps1 -Out <folder>, or -Check <folder>"
    exit 2
}

# The Firefox manifest, as the object ConvertFrom-Json makes.
function Get-FirefoxManifest {
    $manifest = Get-Content -Raw (Join-Path $source "manifest.json") | ConvertFrom-Json
    $changes = Get-Content -Raw (Join-Path $source "firefox.json") | ConvertFrom-Json
    foreach ($change in $changes.PSObject.Properties) {
        $manifest.PSObject.Properties.Remove($change.Name)
        if ($null -ne $change.Value) {
            $manifest | Add-Member -NotePropertyName $change.Name -NotePropertyValue $change.Value
        }
    }
    $manifest
}

# Every file under $Dir, as paths relative to it with "/" between folders,
# sorted, leaving out the top-level names in $Skip.
function Get-RelativeFiles([string]$Dir, [string[]]$Skip = @()) {
    $root = (Resolve-Path $Dir).Path.TrimEnd('\', '/')
    Get-ChildItem -Path $root -Recurse -File -Force |
        ForEach-Object { $_.FullName.Substring($root.Length + 1).Replace('\', '/') } |
        Where-Object { $Skip -notcontains $_.Split('/')[0] } |
        Sort-Object -CaseSensitive
}

# A parsed JSON value as compact JSON text with the keys of every object
# sorted, so two manifests compare equal whatever their spacing, key order
# or escapes.
function ConvertTo-CanonicalJson($Value) {
    if ($null -eq $Value) { return "null" }
    if ($Value -is [string]) { return ($Value | ConvertTo-Json -Compress) }
    if ($Value -is [bool]) { return $Value.ToString().ToLowerInvariant() }
    if ($Value -is [System.Collections.IList]) {
        return "[" + (($Value | ForEach-Object { ConvertTo-CanonicalJson $_ }) -join ",") + "]"
    }
    if ($Value -is [System.Management.Automation.PSCustomObject]) {
        $pairs = $Value.PSObject.Properties | Sort-Object Name -CaseSensitive |
            ForEach-Object { ($_.Name | ConvertTo-Json -Compress) + ":" + (ConvertTo-CanonicalJson $_.Value) }
        return "{" + ($pairs -join ",") + "}"
    }
    return [string]$Value
}

if ($Check) {
    if (-not (Test-Path $Check)) {
        Write-Host "$Check does not exist"
        exit 1
    }
    $differences = New-Object System.Collections.Generic.List[string]
    $found = New-Object System.Collections.Generic.HashSet[string]
    foreach ($file in Get-RelativeFiles $Check) { $found.Add($file) | Out-Null }
    foreach ($file in Get-RelativeFiles $source $skipped) {
        $copy = Join-Path $Check $file
        if (-not $found.Contains($file)) {
            $differences.Add("missing: $file")
        } else {
            $want = [IO.File]::ReadAllBytes((Join-Path $source $file))
            $have = [IO.File]::ReadAllBytes($copy)
            if ([Convert]::ToBase64String($want) -ne [Convert]::ToBase64String($have)) {
                $differences.Add("changed: $file")
            }
            $found.Remove($file) | Out-Null
        }
    }
    if (-not $found.Contains("manifest.json")) {
        $differences.Add("missing: manifest.json")
    } else {
        $found.Remove("manifest.json") | Out-Null
        $manifest = $null
        try {
            $manifest = Get-Content -Raw (Join-Path $Check "manifest.json") | ConvertFrom-Json
        } catch {
            $differences.Add("changed: manifest.json is not JSON ($($_.Exception.Message))")
        }
        if ($null -ne $manifest) {
            $want = Get-FirefoxManifest
            $keys = @($want.PSObject.Properties.Name) + @($manifest.PSObject.Properties.Name) |
                Sort-Object -Unique -CaseSensitive
            foreach ($key in $keys) {
                if ((ConvertTo-CanonicalJson $want.$key) -cne (ConvertTo-CanonicalJson $manifest.$key)) {
                    $differences.Add("changed: manifest.json key `"$key`"")
                }
            }
        }
    }
    foreach ($file in $found | Sort-Object -CaseSensitive) { $differences.Add("extra: $file") }
    if ($differences.Count) {
        Write-Host "$Check is not the Firefox copy of the current extension:"
        foreach ($d in $differences) { Write-Host "  $d" }
        Write-Host "Remake it with: .\installer\make-firefox.ps1 -Out $Check"
        exit 1
    }
    Write-Host "$Check is up to date with the extension"
    exit 0
}

if (Test-Path $Out) {
    # Only ever replace an earlier copy, never some other folder.
    if ((Get-ChildItem $Out) -and -not (Test-Path (Join-Path $Out "manifest.json"))) {
        throw "$Out is not empty and holds no extension"
    }
    Remove-Item -Recurse -Force $Out
}
New-Item -ItemType Directory -Force $Out | Out-Null
Get-ChildItem $source | Where-Object { $_.Name -notin $skipped } |
    Copy-Item -Destination $Out -Recurse

# Windows PowerShell 5.1 writes some arrays as {"value": [...], "Count": n}
# because of its extra type data for arrays; without it they stay arrays.
if ($PSVersionTable.PSVersion.Major -lt 6) {
    Remove-TypeData System.Array -ErrorAction SilentlyContinue
}
# UTF-8 without a byte order mark, which Windows PowerShell's Set-Content
# would add.
$path = Join-Path (Resolve-Path $Out) "manifest.json"
[IO.File]::WriteAllText($path, ((Get-FirefoxManifest) | ConvertTo-Json -Depth 10))
Write-Host "Wrote the Firefox extension to $Out"
