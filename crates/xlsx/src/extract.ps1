$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$maxEntries = 10000
$maxEntryBytes = 50MB
$maxExpandedBytes = 200MB
$maxSheets = 100
$maxCellsPerSheet = 250000
$maxCellsTotal = 500000

function Emit([string]$line) {
    [Console]::Out.WriteLine($line)
}

function Encode([string]$value) {
    if ($null -eq $value) { $value = "" }
    return [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($value))
}

function Read-SecureXml([string]$text) {
    $settings = [System.Xml.XmlReaderSettings]::new()
    $settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
    $settings.XmlResolver = $null
    $reader = [System.Xml.XmlReader]::Create([IO.StringReader]::new($text), $settings)
    try {
        $document = [System.Xml.XmlDocument]::new()
        $document.XmlResolver = $null
        $document.Load($reader)
        return $document
    } finally {
        $reader.Dispose()
    }
}

function Cell-Position([string]$reference) {
    if ($reference -notmatch '^([A-Za-z]+)([1-9][0-9]*)$') {
        throw "Invalid worksheet cell reference"
    }
    [uint64]$column = 0
    foreach ($character in $Matches[1].ToUpperInvariant().ToCharArray()) {
        $column = $column * 26 + ([int]$character - [int][char]'A' + 1)
    }
    $row = [uint64]::Parse($Matches[2], [Globalization.CultureInfo]::InvariantCulture)
    if ($column -gt [uint32]::MaxValue -or $row -gt [uint32]::MaxValue) {
        throw "Worksheet cell reference is outside the supported range"
    }
    return @([uint32]($row - 1), [uint32]($column - 1))
}

function Text-Content($node) {
    $builder = [Text.StringBuilder]::new()
    foreach ($textNode in $node.SelectNodes(".//*[local-name()='t']")) {
        [void]$builder.Append($textNode.InnerText)
    }
    return $builder.ToString()
}

function Resolve-Part([string]$target) {
    $baseUri = [Uri]::new("http://package/xl/workbook.xml")
    $resolved = [Uri]::new($baseUri, $target.Replace('\', '/'))
    $part = [Uri]::UnescapeDataString($resolved.AbsolutePath.TrimStart('/'))
    if (-not $part.StartsWith("xl/", [StringComparison]::OrdinalIgnoreCase)) {
        throw "Workbook relationship points outside the xl package tree"
    }
    return $part.ToLowerInvariant()
}

$path = $env:COMMERCIAL_PREFLIGHT_XLSX_PATH
if ([string]::IsNullOrWhiteSpace($path)) {
    throw "Workbook path was not provided"
}
$stream = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
try {
    $archive = [IO.Compression.ZipArchive]::new(
        $stream,
        [IO.Compression.ZipArchiveMode]::Read,
        $false
    )
    try {
        if ($archive.Entries.Count -gt $maxEntries) {
            throw "Workbook has too many archive entries"
        }
        $entries = @{}
        [uint64]$expanded = 0
        foreach ($entry in $archive.Entries) {
            if ($entry.Length -gt $maxEntryBytes) {
                throw "Workbook archive entry is too large"
            }
            $expanded += [uint64]$entry.Length
            if ($expanded -gt $maxExpandedBytes) {
                throw "Workbook expanded package is too large"
            }
            $name = $entry.FullName.Replace('\', '/').ToLowerInvariant()
            if ($entries.ContainsKey($name)) {
                throw "Workbook contains duplicate archive entry names"
            }
            if ($name.EndsWith("vbaproject.bin", [StringComparison]::OrdinalIgnoreCase)) {
                throw "Macro-enabled workbook content is not supported"
            }
            $entries[$name] = $entry
        }

        function Entry-Text([string]$name) {
            $key = $name.Replace('\', '/').ToLowerInvariant()
            if (-not $entries.ContainsKey($key)) {
                throw "Required workbook package part is missing"
            }
            $entryStream = $entries[$key].Open()
            try {
                $reader = [IO.StreamReader]::new(
                    $entryStream,
                    [Text.UTF8Encoding]::new($false, $true),
                    $true
                )
                try { return $reader.ReadToEnd() } finally { $reader.Dispose() }
            } finally {
                $entryStream.Dispose()
            }
        }

        if (-not $entries.ContainsKey("[content_types].xml") -or
            -not $entries.ContainsKey("xl/workbook.xml") -or
            -not $entries.ContainsKey("xl/_rels/workbook.xml.rels")) {
            throw "Required workbook package part is missing"
        }

        $workbook = Read-SecureXml (Entry-Text "xl/workbook.xml")
        $relationships = Read-SecureXml (Entry-Text "xl/_rels/workbook.xml.rels")
        $relationshipMap = @{}
        foreach ($relationship in $relationships.SelectNodes("//*[local-name()='Relationship']")) {
            $relationshipMap[$relationship.GetAttribute("Id")] =
                Resolve-Part $relationship.GetAttribute("Target")
        }

        $sharedStrings = @()
        if ($entries.ContainsKey("xl/sharedstrings.xml")) {
            $sharedXml = Read-SecureXml (Entry-Text "xl/sharedstrings.xml")
            $sharedStrings = @(
                foreach ($item in $sharedXml.SelectNodes("//*[local-name()='si']")) {
                    Text-Content $item
                }
            )
        }

        $dateStyles = @{}
        if ($entries.ContainsKey("xl/styles.xml")) {
            $styles = Read-SecureXml (Entry-Text "xl/styles.xml")
            $customDateFormats = @{}
            foreach ($format in $styles.SelectNodes("//*[local-name()='numFmt']")) {
                $code = $format.GetAttribute("formatCode")
                $withoutLiterals = [Regex]::Replace($code, '"[^"]*"|\[[^\]]*\]', "")
                if ($withoutLiterals -match '[ymdhis]') {
                    $customDateFormats[[int]$format.GetAttribute("numFmtId")] = $true
                }
            }
            $styleIndex = 0
            $cellXfs = $styles.SelectSingleNode("//*[local-name()='cellXfs']")
            if ($null -ne $cellXfs) {
                foreach ($xf in $cellXfs.SelectNodes("./*[local-name()='xf']")) {
                    $formatId = 0
                    if ($xf.HasAttribute("numFmtId")) {
                        $formatId = [int]$xf.GetAttribute("numFmtId")
                    }
                    if (($formatId -ge 14 -and $formatId -le 22) -or
                        ($formatId -ge 45 -and $formatId -le 47) -or
                        $customDateFormats.ContainsKey($formatId)) {
                        $dateStyles[$styleIndex] = $true
                    }
                    $styleIndex++
                }
            }
        }

        $date1904 = $false
        $workbookProperties = $workbook.SelectSingleNode("//*[local-name()='workbookPr']")
        if ($null -ne $workbookProperties) {
            $rawDate1904 = $workbookProperties.GetAttribute("date1904")
            $date1904 = $rawDate1904 -eq "1" -or $rawDate1904 -eq "true"
        }

        Emit "PREFLIGHT1"
        if ($entries.ContainsKey("docprops/core.xml")) {
            $core = Read-SecureXml (Entry-Text "docprops/core.xml")
            foreach ($name in @("title", "subject", "description", "keywords", "category")) {
                $node = $core.SelectSingleNode("//*[local-name()='$name']")
                if ($null -ne $node -and -not [string]::IsNullOrEmpty($node.InnerText)) {
                    Emit ("M`t{0}`t{1}" -f $name, (Encode $node.InnerText))
                }
            }
        }

        $sheets = @($workbook.SelectNodes("//*[local-name()='sheets']/*[local-name()='sheet']"))
        if ($sheets.Count -eq 0) {
            throw "Workbook contains no worksheets"
        }
        if ($sheets.Count -gt $maxSheets) {
            throw "Workbook has too many worksheets"
        }
        $totalCells = 0
        foreach ($sheet in $sheets) {
            $relationshipId = $sheet.GetAttribute(
                "id",
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            )
            if (-not $relationshipMap.ContainsKey($relationshipId)) {
                throw "Worksheet relationship is missing"
            }
            $part = $relationshipMap[$relationshipId]
            if (-not $part.StartsWith("xl/worksheets/", [StringComparison]::OrdinalIgnoreCase)) {
                throw "Worksheet relationship has an invalid target"
            }
            $state = $sheet.GetAttribute("state")
            $visibility = switch ($state) {
                "hidden" { "H" }
                "veryHidden" { "X" }
                default { "V" }
            }
            Emit ("S`t{0}`t{1}" -f $visibility, (Encode $sheet.GetAttribute("name")))
            $worksheet = Read-SecureXml (Entry-Text $part)
            $sheetCells = 0
            foreach ($cell in $worksheet.SelectNodes("//*[local-name()='c']")) {
                $sheetCells++
                $totalCells++
                if ($sheetCells -gt $maxCellsPerSheet -or $totalCells -gt $maxCellsTotal) {
                    throw "Workbook has too many populated cells"
                }
                $position = Cell-Position $cell.GetAttribute("r")
                $formula = $cell.SelectSingleNode("./*[local-name()='f']")
                if ($null -ne $formula) {
                    Emit ("C`t{0}`t{1}`tF`t" -f $position[0], $position[1])
                    continue
                }
                $type = $cell.GetAttribute("t")
                $valueNode = $cell.SelectSingleNode("./*[local-name()='v']")
                $payload = ""
                $kind = $null
                switch ($type) {
                    "s" {
                        if ($null -eq $valueNode) { continue }
                        $sharedIndex = [int]::Parse(
                            $valueNode.InnerText,
                            [Globalization.CultureInfo]::InvariantCulture
                        )
                        if ($sharedIndex -lt 0 -or $sharedIndex -ge $sharedStrings.Count) {
                            throw "Shared string index is outside the table"
                        }
                        $kind = "T"
                        $payload = $sharedStrings[$sharedIndex]
                    }
                    "inlineStr" {
                        $kind = "T"
                        $payload = Text-Content $cell
                    }
                    "str" {
                        if ($null -eq $valueNode) { continue }
                        $kind = "T"
                        $payload = $valueNode.InnerText
                    }
                    "b" {
                        if ($null -eq $valueNode) { continue }
                        $kind = "B"
                        $payload = if ($valueNode.InnerText -eq "1") { "1" } else { "0" }
                    }
                    "e" {
                        $kind = "X"
                    }
                    "d" {
                        if ($null -eq $valueNode) { continue }
                        $kind = "D"
                        $payload = ([DateTime]::Parse(
                            $valueNode.InnerText,
                            [Globalization.CultureInfo]::InvariantCulture,
                            [Globalization.DateTimeStyles]::RoundtripKind
                        )).ToString(
                            "yyyy-MM-dd",
                            [Globalization.CultureInfo]::InvariantCulture
                        )
                    }
                    default {
                        if ($null -eq $valueNode -or
                            [string]::IsNullOrWhiteSpace($valueNode.InnerText)) {
                            continue
                        }
                        $number = [double]::Parse(
                            $valueNode.InnerText,
                            [Globalization.CultureInfo]::InvariantCulture
                        )
                        $style = 0
                        if ($cell.HasAttribute("s")) {
                            $style = [int]$cell.GetAttribute("s")
                        }
                        if ($dateStyles.ContainsKey($style)) {
                            if ($date1904) { $number += 1462 }
                            $kind = "D"
                            $payload = [DateTime]::FromOADate($number).ToString(
                                "yyyy-MM-dd",
                                [Globalization.CultureInfo]::InvariantCulture
                            )
                        } else {
                            $kind = "N"
                            $payload = $number.ToString(
                                "R",
                                [Globalization.CultureInfo]::InvariantCulture
                            )
                        }
                    }
                }
                if ($null -eq $kind) {
                    continue
                }
                Emit ("C`t{0}`t{1}`t{2}`t{3}" -f
                    $position[0],
                    $position[1],
                    $kind,
                    (Encode $payload)
                )
            }
            Emit "E"
        }
    } finally {
        $archive.Dispose()
    }
} finally {
    $stream.Dispose()
}
