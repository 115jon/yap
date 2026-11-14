# Shared by package-toolchain.ps1 and fetch-toolchain.ps1 (dot-sourced).

# SHA-256 and length of many files, hashed in parallel (one runspace per
# core; files spread across them biggest first). $Items have Rel, Path and
# Length (the size on disk, for the spreading only).
# With $Needles, each result's Leak is true when the file contains one of
# them (case-insensitive bytes, matched across buffer boundaries).
# Returns a hashtable Rel -> [pscustomobject]@{ Length; Hash; Leak }.
function Get-FileDigests([object[]]$Items, [string[]]$Needles = @()) {
    $n = [Environment]::ProcessorCount
    $buckets = [object[]]::new($n)
    for ($i = 0; $i -lt $n; $i++) { $buckets[$i] = [Collections.Generic.List[object]]::new() }
    $i = 0
    foreach ($it in $Items | Sort-Object Length -Descending) {
        $buckets[$i % $n].Add($it); $i++
    }
    $out = @{}
    $buckets | Where-Object Count | ForEach-Object -ThrottleLimit $n -Parallel {
        $needles = $using:Needles
        $buf = [byte[]]::new(4MB)
        foreach ($it in $_) {
            $sha = [Security.Cryptography.IncrementalHash]::CreateHash([Security.Cryptography.HashAlgorithmName]::SHA256)
            $fs = [IO.FileStream]::new($it.Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read, 1, [IO.FileOptions]::SequentialScan)
            $len = [long]0; $tail = ""; $leak = $false
            try {
                while (($r = $fs.Read($buf, 0, $buf.Length)) -gt 0) {
                    $sha.AppendData($buf, 0, $r); $len += $r
                    if ($needles -and -not $leak) {
                        $text = $tail + [Text.Encoding]::Latin1.GetString($buf, 0, $r)
                        foreach ($nd in $needles) {
                            if ($text.IndexOf($nd, [StringComparison]::OrdinalIgnoreCase) -ge 0) { $leak = $true; break }
                        }
                        $tail = $text.Substring([Math]::Max(0, $text.Length - 256))
                    }
                }
                [pscustomobject]@{ Rel = $it.Rel; Length = $len; Hash = [Convert]::ToHexString($sha.GetHashAndReset()).ToLowerInvariant(); Leak = $leak }
            } finally { $fs.Dispose(); $sha.Dispose() }
        }
    } | ForEach-Object { $out[$_.Rel] = $_ }
    return $out
}

# SHA-256 of an empty file: no packed library or object may have it.
$EmptySha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
