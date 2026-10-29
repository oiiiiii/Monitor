# ========================================
#  Monitor 单机版 - Windows 发布打包脚本
# ========================================

param(
    [string]$Version = "0.1.0",
    [string]$OutputDir = "./release"
)

$ErrorActionPreference = "Stop"

Write-Host ""
Write-Host " ========================================" -ForegroundColor Cyan
Write-Host "  Monitor 单机版 - Windows 发布打包" -ForegroundColor Cyan
Write-Host " ========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host " 版本: $Version" -ForegroundColor Yellow
Write-Host " 输出: $OutputDir" -ForegroundColor Yellow
Write-Host ""

# 检查 Rust 环境
if (-not (Get-Command "cargo" -ErrorAction SilentlyContinue)) {
    Write-Host "[错误] 未找到 cargo，请先安装 Rust" -ForegroundColor Red
    exit 1
}

# 构建 Release 版本
Write-Host "[1/5] 编译 Release 版本..." -ForegroundColor Green
cargo build --release -p monitor-standalone
if ($LASTEXITCODE -ne 0) {
    Write-Host "[错误] 编译失败" -ForegroundColor Red
    exit 1
}

# 创建输出目录
$PackageName = "Monitor-Standalone-$Version-win-x64"
$PackageDir = Join-Path $OutputDir $PackageName

Write-Host "[2/5] 创建发布目录: $PackageDir" -ForegroundColor Green
if (Test-Path $PackageDir) {
    Remove-Item -Recurse -Force $PackageDir
}
New-Item -ItemType Directory -Force -Path $PackageDir | Out-Null

# 复制二进制文件
Write-Host "[3/5] 复制程序文件..." -ForegroundColor Green
Copy-Item "target\release\monitor.exe" (Join-Path $PackageDir "monitor.exe")

# 复制配置和脚本
Write-Host "[4/5] 复制配置和脚本..." -ForegroundColor Green
$StandaloneDir = "deploy\standalone"
Copy-Item (Join-Path $StandaloneDir "config.toml") (Join-Path $PackageDir "config.toml")
Copy-Item (Join-Path $StandaloneDir "启动.bat") (Join-Path $PackageDir "启动.bat")
Copy-Item (Join-Path $StandaloneDir "健康检查.bat") (Join-Path $PackageDir "健康检查.bat")
Copy-Item (Join-Path $StandaloneDir "README.txt") (Join-Path $PackageDir "README.txt")

# 创建空目录
New-Item -ItemType Directory -Force -Path (Join-Path $PackageDir "data") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $PackageDir "logs") | Out-Null

# 打包 zip
Write-Host "[5/5] 打包 ZIP..." -ForegroundColor Green
$ZipPath = Join-Path $OutputDir "$PackageName.zip"
if (Test-Path $ZipPath) {
    Remove-Item -Force $ZipPath
}
Compress-Archive -Path "$PackageDir\*" -DestinationPath $ZipPath

# 显示结果
$ExeSize = (Get-Item (Join-Path $PackageDir "monitor.exe")).Length / 1MB
$ZipSize = (Get-Item $ZipPath).Length / 1MB

Write-Host ""
Write-Host " ========================================" -ForegroundColor Cyan
Write-Host "  打包完成！" -ForegroundColor Green
Write-Host " ========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host " 发布目录: $PackageDir" -ForegroundColor White
Write-Host " 压缩包:   $ZipPath" -ForegroundColor White
Write-Host ""
Write-Host " 程序大小: $($ExeSize.ToString('0.00')) MB" -ForegroundColor Yellow
Write-Host " 压缩大小: $($ZipSize.ToString('0.00')) MB" -ForegroundColor Yellow
Write-Host ""
Write-Host " 使用方法: 解压后双击 启动.bat" -ForegroundColor White
Write-Host ""
