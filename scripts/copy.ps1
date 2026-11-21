param(
    [Parameter(Position = 0)]
    [string]$Path = ".",

    [Parameter(Position = 1, ValueFromRemainingArguments = $true)]
    [string[]]$Ext
)

# 입력받은 확장자 정리 (예: "py", ".py", "py,js" -> ".py", ".js")
$TargetExts = @()
if ($Ext) {
    $TargetExts = $Ext -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" } | ForEach-Object {
        if ($_.StartsWith(".")) { $_.ToLower() } else { ".$($_.ToLower())" }
    }
}

# 파일 탐색 및 확장자 필터링
$Files = Get-ChildItem -Path $Path -Recurse -File | Where-Object {
    $TargetExts.Count -eq 0 -or ($TargetExts -contains $_.Extension.ToLower())
}

if ($Files.Count -eq 0) {
    Write-Host "No files found! (Path: $Path)" -ForegroundColor Yellow
    exit
}

# 내용 합쳐서 클립보드로 복사
$Files | ForEach-Object {
    "=== File: $($_.FullName) ==="
    Get-Content $_.FullName -Raw -Encoding UTF8
    ""
} | Set-Clipboard

$ExtDisplay = if ($TargetExts.Count -eq 0) { "ALL" } else { $TargetExts -join ", " }
Write-Host "Copied $($Files.Count) file(s) to clipboard! (Path: $Path | Ext: $ExtDisplay)" -ForegroundColor Green
