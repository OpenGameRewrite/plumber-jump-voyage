param([Parameter(Mandatory=$true)][string]$Toolbox)
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskToolbox = (Resolve-Path -LiteralPath $Toolbox).Path
$taskOutput = Join-Path $taskRoot 'local\converter'
New-Item -ItemType Directory -Path $taskOutput -Force | Out-Null
$taskNames = @('Toolbox.Library.dll','FirstPlugin.Plg.dll','AssimpNet.dll','OpenTK.dll','Gl_EditorFramework.dll')
$taskRefs = @('/r:System.Windows.Forms.dll','/r:System.Drawing.dll','/r:System.Core.dll')
foreach ($taskName in $taskNames) {
    $taskDll = Get-ChildItem -LiteralPath $taskToolbox -Filter $taskName -Recurse | Select-Object -First 1
    if (!$taskDll) { throw "Missing installed tool dependency: $taskName" }
    $taskRefs += '/r:' + $taskDll.FullName
}
$taskCompiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
& $taskCompiler /nologo /platform:x64 ('/out:' + (Join-Path $taskOutput 'convert.exe')) @taskRefs (Join-Path $PSScriptRoot 'convert.cs')
if ($LASTEXITCODE) { throw 'Converter compilation failed' }
# Assemblies resolve beside the adapter; files remain ignored and local.
Get-ChildItem -LiteralPath $taskToolbox -File -Filter '*.dll' | Copy-Item -Destination $taskOutput -Force
Get-ChildItem -LiteralPath (Join-Path $taskToolbox 'Lib') -File -Filter '*.dll' | Copy-Item -Destination $taskOutput -Force
Get-ChildItem -LiteralPath (Join-Path $taskToolbox 'Lib\Plugins') -File -Filter '*.dll' | Copy-Item -Destination $taskOutput -Force
Get-ChildItem -LiteralPath (Join-Path $taskToolbox 'runtimes\win-x64\native') -File -Filter '*.dll' | Copy-Item -Destination $taskOutput -Force
$taskPluginDir = Join-Path $taskOutput 'Lib\Plugins'
New-Item -ItemType Directory -Path $taskPluginDir -Force | Out-Null
Get-ChildItem -LiteralPath $taskToolbox -File -Filter '*.Plg.dll' | Copy-Item -Destination $taskPluginDir -Force
Write-Output "Built local adapter: $taskOutput\convert.exe"
