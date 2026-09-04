param(
  [string]$JdkHome = $env:JAVA_HOME
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($JdkHome)) {
  throw "JdkHome or JAVA_HOME must point to a JDK 21 installation."
}

$jlink = Join-Path $JdkHome "bin\jlink.exe"
if (-not (Test-Path -LiteralPath $jlink -PathType Leaf)) {
  throw "jlink.exe was not found under the selected JDK. Use a JDK 21 installation."
}

$previousErrorAction = $ErrorActionPreference
$ErrorActionPreference = "Continue"
$javaVersionOutput = @(& (Join-Path $JdkHome "bin\java.exe") -version 2>&1)
$javaExitCode = $LASTEXITCODE
$javaVersion = $javaVersionOutput | Select-Object -First 1
$ErrorActionPreference = $previousErrorAction
if ($javaExitCode -ne 0) {
  throw "The selected Java runtime could not report its version."
}
if ($javaVersion -notmatch 'version "21[\.]') {
  throw "DataNexa JDBC release runtime requires JDK 21. Detected: $javaVersion"
}

& mvn.cmd -q -f (Join-Path $repoRoot "jdbc-sidecar\pom.xml") package
if ($LASTEXITCODE -ne 0) {
  throw "JDBC sidecar Maven build failed."
}

$runtimeRoot = Join-Path $repoRoot "resources\jdbc-runtime"
$runtimeParent = Split-Path -Parent $runtimeRoot
$null = New-Item -ItemType Directory -Path $runtimeParent -Force
$generatedNames = @("bin", "conf", "include", "legal", "lib", "release")
$buildParent = Join-Path ([System.IO.Path]::GetTempPath()) ("datanexa-jdbc-runtime-" + [guid]::NewGuid().ToString("N"))
$buildRuntime = Join-Path $buildParent "runtime"
New-Item -ItemType Directory -Path $buildParent -Force | Out-Null
try {
  & $jlink `
    --add-modules java.base,java.sql,java.naming,java.logging,java.xml,java.management,java.desktop,java.security.jgss,jdk.crypto.ec,jdk.unsupported `
    --strip-debug `
    --no-header-files `
    --no-man-pages `
    --compress=2 `
    --output $buildRuntime
  if ($LASTEXITCODE -ne 0) {
    throw "jlink failed to create the DataNexa JDBC runtime."
  }

  foreach ($name in $generatedNames) {
    $target = Join-Path $runtimeRoot $name
    if (Test-Path -LiteralPath $target) {
      Remove-Item -LiteralPath $target -Recurse -Force
    }
  }
  New-Item -ItemType Directory -Path $runtimeRoot -Force | Out-Null
  Get-ChildItem -LiteralPath $buildRuntime -Force | Move-Item -Destination $runtimeRoot -Force
  $sidecarTarget = Join-Path $runtimeRoot "lib\datanexa-jdbc-sidecar.jar"
  Copy-Item -LiteralPath (Join-Path $repoRoot "jdbc-sidecar\target\datanexa-jdbc-sidecar.jar") -Destination $sidecarTarget -Force
} finally {
  if (Test-Path -LiteralPath $buildParent) {
    Remove-Item -LiteralPath $buildParent -Recurse -Force
  }
}
Write-Host "Prepared DataNexa JDBC runtime at $runtimeRoot"
