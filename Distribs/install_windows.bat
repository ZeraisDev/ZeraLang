@echo off
echo =======================================
echo     Zeralang Installer for Windows
echo =======================================

:: 1. Create installation directory
echo Installing to C:\Zeralang...
mkdir "C:\Zeralang" 2>nul

:: 2. Copy the binary (make sure zera_lang.exe is in the same folder as this script)
copy /Y "zera_lang.exe" "C:\Zeralang\zera.exe" >nul

:: 3. Add to System PATH (User level, doesn't require Admin)
echo Adding Zeralang to PATH...
setx PATH "%PATH%;C:\Zeralang"

:: 4. Associate .zera files with the interpreter
echo Associating .zera files...
assoc .zera=ZeralangScript
ftype ZeralangScript="C:\Zeralang\zera.exe" "%%1"

echo.
echo =======================================
echo Installation Complete!
echo.
echo NOTE: You must close this window and open
echo a NEW Command Prompt for 'zera' to work.
echo =======================================
pause