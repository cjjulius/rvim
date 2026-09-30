# Convenience launcher for rvim (PowerShell).
# Prefers the release binary; falls back to `cargo run`.
$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$bin  = Join-Path $here 'target\release\rvim.exe'

if (Test-Path $bin) {
    & $bin @args
} else {
    cargo run --release --manifest-path (Join-Path $here 'Cargo.toml') -- @args
}
