# PowerShell sample for rvim syntax highlighting.

<#
  Block comment: summarise what the script does.
  Spans multiple lines until the closing marker.
#>

function Get-Average {
    param(
        [double[]]$Numbers,
        [switch]$Round
    )

    if ($Numbers.Count -eq 0) {
        throw "no numbers given"
    }

    $total = 0.0
    foreach ($n in $Numbers) {
        $total = $total + $n
    }

    $avg = $total / $Numbers.Count
    if ($Round) {
        $avg = [int]$avg
    }
    return $avg
}

$values = @(3, 7, 11, 5)
$result = Get-Average -Numbers $values -Round
Write-Output "average: $result"

# Built-in cmdlets are highlighted as single Verb-Noun tokens.
Get-ChildItem -Path . | Where-Object { $_.Length -gt 0 } | Sort-Object Name
