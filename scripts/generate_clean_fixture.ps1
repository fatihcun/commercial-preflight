param(
    [string]$Source = "tests/fixtures/Project_Orion_Quote_v0.1.xlsx",
    [string]$Destination = "tests/fixtures/Project_Orion_Quote_clean.xlsx"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$root = Split-Path -Parent $PSScriptRoot
$sourcePath = [IO.Path]::GetFullPath((Join-Path $root $Source))
$destinationPath = [IO.Path]::GetFullPath((Join-Path $root $Destination))
if (-not [IO.File]::Exists($sourcePath)) {
    throw "Source fixture does not exist: $sourcePath"
}
if ($sourcePath -eq $destinationPath) {
    throw "Source and destination fixtures must be different files."
}

[IO.File]::Copy($sourcePath, $destinationPath, $true)

function Read-EntryText(
    [IO.Compression.ZipArchive]$Archive,
    [string]$Name
) {
    $entry = $Archive.GetEntry($Name)
    if ($null -eq $entry) {
        throw "Required package entry is missing: $Name"
    }
    $reader = [IO.StreamReader]::new(
        $entry.Open(),
        [Text.UTF8Encoding]::new($false, $true)
    )
    try {
        return $reader.ReadToEnd()
    } finally {
        $reader.Dispose()
    }
}

function Write-EntryText(
    [IO.Compression.ZipArchive]$Archive,
    [string]$Name,
    [string]$Text
) {
    $existing = $Archive.GetEntry($Name)
    if ($null -ne $existing) {
        $existing.Delete()
    }
    $entry = $Archive.CreateEntry(
        $Name,
        [IO.Compression.CompressionLevel]::Optimal
    )
    $writer = [IO.StreamWriter]::new(
        $entry.Open(),
        [Text.UTF8Encoding]::new($false)
    )
    try {
        $writer.Write($Text)
    } finally {
        $writer.Dispose()
    }
}

function Set-CellValue(
    [xml]$Worksheet,
    [string]$Reference,
    [string]$Value
) {
    $cell = $Worksheet.SelectSingleNode(
        "//*[local-name()='c' and @r='$Reference']"
    )
    if ($null -eq $cell) {
        throw "Fixture cell is missing: $Reference"
    }
    $valueNode = $cell.SelectSingleNode("./*[local-name()='v']")
    if ($null -eq $valueNode) {
        $valueNode = $cell.SelectSingleNode(".//*[local-name()='t']")
    }
    if ($null -eq $valueNode) {
        throw "Fixture cell has no editable value: $Reference"
    }
    $valueNode.InnerText = $Value
}

$stream = [IO.File]::Open(
    $destinationPath,
    [IO.FileMode]::Open,
    [IO.FileAccess]::ReadWrite,
    [IO.FileShare]::None
)
try {
    $archive = [IO.Compression.ZipArchive]::new(
        $stream,
        [IO.Compression.ZipArchiveMode]::Update,
        $false
    )
    try {
        [xml]$offer = Read-EntryText $archive "xl/worksheets/sheet1.xml"
        Set-CellValue $offer "F15" "0.10"
        Set-CellValue $offer "B9" "60 days"
        Set-CellValue $offer "B8" "46355"
        Set-CellValue $offer "H25" "220770"
        Write-EntryText $archive "xl/worksheets/sheet1.xml" $offer.OuterXml

        [xml]$workbook = Read-EntryText $archive "xl/workbook.xml"
        $internalSheet = $workbook.SelectSingleNode(
            "//*[local-name()='sheet' and @name='Internal Cost']"
        )
        if ($null -eq $internalSheet) {
            throw "Internal Cost sheet registration is missing."
        }
        [void]$internalSheet.ParentNode.RemoveChild($internalSheet)
        foreach ($definedName in $workbook.SelectNodes(
            "//*[local-name()='definedName' and @localSheetId]"
        )) {
            $sheetIndex = [int]$definedName.GetAttribute("localSheetId")
            if ($sheetIndex -gt 1) {
                $definedName.SetAttribute("localSheetId", ($sheetIndex - 1))
            }
        }
        Write-EntryText $archive "xl/workbook.xml" $workbook.OuterXml

        [xml]$relationships = Read-EntryText $archive "xl/_rels/workbook.xml.rels"
        $internalRelationship = $relationships.SelectSingleNode(
            "//*[local-name()='Relationship' and @Id='rId2']"
        )
        if ($null -eq $internalRelationship) {
            throw "Internal Cost relationship is missing."
        }
        [void]$internalRelationship.ParentNode.RemoveChild($internalRelationship)
        Write-EntryText $archive "xl/_rels/workbook.xml.rels" $relationships.OuterXml

        [xml]$contentTypes = Read-EntryText $archive "[Content_Types].xml"
        $internalContentType = $contentTypes.SelectSingleNode(
            "//*[local-name()='Override' and @PartName='/xl/worksheets/sheet2.xml']"
        )
        if ($null -ne $internalContentType) {
            [void]$internalContentType.ParentNode.RemoveChild($internalContentType)
        }
        Write-EntryText $archive "[Content_Types].xml" $contentTypes.OuterXml

        $internalEntry = $archive.GetEntry("xl/worksheets/sheet2.xml")
        if ($null -eq $internalEntry) {
            throw "Internal Cost worksheet entry is missing."
        }
        $internalEntry.Delete()

        [xml]$properties = Read-EntryText $archive "docProps/core.xml"
        $metadata = @{
            subject = "Example Energy Commercial Proposal"
            description = "Synthetic clean regression fixture."
            keywords = "proposal, Example Energy"
        }
        foreach ($name in $metadata.Keys) {
            $node = $properties.SelectSingleNode("//*[local-name()='$name']")
            if ($null -eq $node) {
                throw "Core property is missing: $name"
            }
            $node.InnerText = $metadata[$name]
        }
        Write-EntryText $archive "docProps/core.xml" $properties.OuterXml
    } finally {
        $archive.Dispose()
    }
} finally {
    $stream.Dispose()
}

Write-Output "Generated $destinationPath"
