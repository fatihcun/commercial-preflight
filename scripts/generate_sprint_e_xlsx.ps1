param(
    [string]$Destination = "tests/fixtures/SprintE_Quote_Q-2026-184_Rev3.xlsx"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$root = Split-Path -Parent $PSScriptRoot
$destinationPath = [IO.Path]::GetFullPath((Join-Path $root $Destination))
$parent = Split-Path -Parent $destinationPath
[IO.Directory]::CreateDirectory($parent) | Out-Null
if ([IO.File]::Exists($destinationPath)) {
    [IO.File]::Delete($destinationPath)
}

function Add-TextEntry(
    [IO.Compression.ZipArchive]$Archive,
    [string]$Name,
    [string]$Text
) {
    $entry = $Archive.CreateEntry($Name, [IO.Compression.CompressionLevel]::Optimal)
    $writer = [IO.StreamWriter]::new($entry.Open(), [Text.UTF8Encoding]::new($false))
    try { $writer.Write($Text) } finally { $writer.Dispose() }
}

$contentTypes = @'
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>
'@
$rootRels = @'
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>
'@
$workbook = @'
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets><sheet name="Quote" sheetId="1" r:id="rId1"/></sheets>
</workbook>
'@
$workbookRels = @'
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>
'@
$worksheet = @'
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <dimension ref="A1:G15"/>
  <sheetData>
    <row r="1"><c r="A1" t="inlineStr"><is><t>Customer</t></is></c><c r="B1" t="inlineStr"><is><t>Example Electrical</t></is></c></row>
    <row r="2"><c r="A2" t="inlineStr"><is><t>Quote No</t></is></c><c r="B2" t="inlineStr"><is><t>Q-2026-184</t></is></c></row>
    <row r="3"><c r="A3" t="inlineStr"><is><t>Quote Revision</t></is></c><c r="B3" t="inlineStr"><is><t>3</t></is></c></row>
    <row r="4"><c r="A4" t="inlineStr"><is><t>Quote Date</t></is></c><c r="B4" t="inlineStr"><is><t>2026-10-20</t></is></c></row>
    <row r="5"><c r="A5" t="inlineStr"><is><t>Currency</t></is></c><c r="B5" t="inlineStr"><is><t>EUR</t></is></c></row>
    <row r="6"><c r="A6" t="inlineStr"><is><t>Payment Term</t></is></c><c r="B6" t="inlineStr"><is><t>30 days</t></is></c></row>
    <row r="7"><c r="A7" t="inlineStr"><is><t>Delivery</t></is></c><c r="B7" t="inlineStr"><is><t>3 weeks</t></is></c></row>
    <row r="9"><c r="A9" t="inlineStr"><is><t>SKU</t></is></c><c r="B9" t="inlineStr"><is><t>Description</t></is></c><c r="C9" t="inlineStr"><is><t>Quantity</t></is></c><c r="D9" t="inlineStr"><is><t>UOM</t></is></c><c r="E9" t="inlineStr"><is><t>Unit Price</t></is></c><c r="F9" t="inlineStr"><is><t>Discount</t></is></c><c r="G9" t="inlineStr"><is><t>Line Total</t></is></c></row>
    <row r="10"><c r="A10" t="inlineStr"><is><t>FG-200G</t></is></c><c r="B10" t="inlineStr"><is><t>Firewall Gateway</t></is></c><c r="C10"><v>2</v></c><c r="D10" t="inlineStr"><is><t>EA</t></is></c><c r="E10"><v>8400</v></c><c r="F10"><v>0</v></c><c r="G10"><v>16800</v></c></row>
    <row r="11"><c r="A11" t="inlineStr"><is><t>SUP-3Y</t></is></c><c r="B11" t="inlineStr"><is><t>Support 3Y</t></is></c><c r="C11"><v>2</v></c><c r="D11" t="inlineStr"><is><t>EA</t></is></c><c r="E11"><v>2100</v></c><c r="F11"><v>0</v></c><c r="G11"><v>4200</v></c></row>
    <row r="13"><c r="A13" t="inlineStr"><is><t>Subtotal</t></is></c><c r="B13"><v>21000</v></c></row>
    <row r="14"><c r="A14" t="inlineStr"><is><t>VAT / Tax</t></is></c><c r="B14"><v>0</v></c></row>
    <row r="15"><c r="A15" t="inlineStr"><is><t>Grand Total</t></is></c><c r="B15"><v>21000</v></c></row>
  </sheetData>
</worksheet>
'@

$stream = [IO.File]::Open($destinationPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
try {
    $archive = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Create, $false)
    try {
        Add-TextEntry $archive "[Content_Types].xml" $contentTypes
        Add-TextEntry $archive "_rels/.rels" $rootRels
        Add-TextEntry $archive "xl/workbook.xml" $workbook
        Add-TextEntry $archive "xl/_rels/workbook.xml.rels" $workbookRels
        Add-TextEntry $archive "xl/worksheets/sheet1.xml" $worksheet
    } finally { $archive.Dispose() }
} finally { $stream.Dispose() }

Write-Output "Generated $destinationPath"
