#!/bin/bash

# Install version 5.82 bluez from source.

echo "This script will install the latest bluez from source"
echo "WARNING: This script has only been tested on"
echo "- Radxa rock images of Debian 11"
echo "- Raspberry Pi images of Debian 12"

while true; do
  echo "Do you want to proceed? (y/n)"
  read -r answer

  if [[ $answer =~ ^[Yy]$ ]]; then
    echo "Proceeding..."
    break
  elif [[ $answer =~ ^[Nn]$ ]]; then
    echo "Exiting..."
    exit 0
  else
    echo "Invalid input. Please enter y or n"
  fi
done

echo "Ensuring deb-src debian repo is in apt sources..."
SOURCE_FILE="/etc/apt/sources.list.d/socks-forwarder.list"
REPO_URL="deb-src https://deb.debian.org/debian bullseye main contrib non-free"
echo "$REPO_URL" | sudo tee "$SOURCE_FILE"

echo "Updating apt..."
sudo apt update

echo "Disabling GATT caching..."
SOURCE_FILE="/etc/bluetooth/main.conf"
DISABLE_GATT_LINE="[GATT]\nCache=no"
echo -e $DISABLE_GATT_LINE | sudo tee -a $SOURCE_FILE

echo "Installing python3-docutils..."
sudo apt install python3-docutils

echo "Installing git (likely already installed)..."
sudo apt install git

set -e

echo "Cloning bluez..."
pushd ~
git clone https://github.com/bluez/bluez.git

pushd bluez
echo "Checking out commit hash of version v5.82..."
# Slightly hacky to checkout a hash, but bluez does not use git tags.
git checkout 0efa20cbf3fb5693c7c2f14ba8cf67053ca029e5
echo "Installing bluez requirements..."
sudo apt-get build-dep bluez
echo "Bootstrapping..."
./bootstrap
echo "Configuring..."
./configure --prefix=/usr --mandir=/usr/share/man --sysconfdir=/etc --localstatedir=/var
echo "Installing bluez (this may take a while)..."
make && sudo make install

popd
popd
echo "Unmasking, enabling, and restarting bluetooth service"
sudo systemctl unmask bluetooth
sudo systemctl enable bluetooth
sudo systemctl restart bluetooth
echo "bluez v5.82 installation successful. Use `bluetoothctl version` to verify version."
echo "Remember to run hciattach commands if using external bluetooth adapters."
