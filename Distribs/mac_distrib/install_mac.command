#!/bin/bash

echo "======================================="
echo "      Zeralang Installer for macOS"
echo "======================================="

# 1. Find the directory where this script is running
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# 2. Copy the binary to /usr/local/bin (requires sudo)
echo "Copying binary to /usr/local/bin/zera (requires admin password)..."
sudo cp "$SCRIPT_DIR/zera_lang" /usr/local/bin/zera
sudo chmod +x /usr/local/bin/zera

# 3. Tell macOS to open .zera files with Zera
echo "Associating .zera files..."
# Bind the extension to the interpreter using macOS defaults
defaults write com.apple.LaunchServices LSHandlers -array-add '<dict><key>LSHandlerContentTag</key><string>zera</string><key>LSHandlerContentTagClass</key><string>public.filename-extension</string><key>LSHandlerRoleAll</key><string>org.zeralang.interpreter</string></dict>'
echo ""
echo "======================================="
echo "Installation Complete!"
echo "Open a new Terminal window and type 'zera' to start the REPL."
echo "======================================="

# Keep the terminal window open so they can read the output
read -p "Press Enter to exit..."