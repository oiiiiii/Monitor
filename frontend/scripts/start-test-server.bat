@echo off
cd /d "%~dp0.."
node scripts/wave-test-server.cjs
pause