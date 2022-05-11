#!/bin/sh

# MITRE's contributions to this script are licensed under the Apache-2.0 license.
# The full text of this license may be found in the `LICENSE.md` file for this
# repository.
#
# Based on the Deno installer shell script, which is MIT licensed:
#
# MIT License
#
# Copyright (c) 2018-2022 the Deno authors
#
# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in all
# copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
# SOFTWARE.

# # How to use this script
#
# ## Description
#
# This script is the installer for Hipcheck. It makes sure you have the tools
# you need to do the install, downloads Hipcheck, builds it, places the
# artifacts into the appropriate directories, and tells you how to update
# your shell environment to make sure Hipcheck runs correctly.
#
# ## Input Environment Variables
#
# The following environment variables set where Hipcheck will place the
# artifacts it needs. If unset, each has a default value.
#
# - `HC_BIN`: where the `hc` binary will be placed.
# - `HC_CONFIG`: where Hipcheck's configuration files will be placed.
# - `HC_DATA`: where Hipcheck's scripts will be placed.

# Shell Configuration
#
# This makes sure we exit if anything has a non-zero exit code.
set -e

# Operating System Detection
#
# This is done with uname to remain purely POSIX-shell compliant.
# If we don't recognize the name, for now we assume Linux.
case $(uname | tr '[:upper:]' '[:lower:]') in
    linux*)
        os_name="linux"
        ;;
    darwin*)
        os_name="macos"
        ;;
    msys* | cygwin*)
        os_name="windows"
        ;;
    *)
        os_name="linux"
        ;;
esac

# Check Prerequisites
if ! command -v cargo >/dev/null; then
    echo "error: unable to find 'cargo'; install Rust (rust-lang.org/tools/install) before continuing." 1>&2
    exit 1
fi
if ! command -v curl >/dev/null; then
    echo "error: unable to find 'curl'; install curl before continuing." 1>&2
    exit 1
fi

# Set Variables
#
# This handles defaulting behavior based on environment variables,
# first preferring anything explicitly set by the user, then defaulting to
# fallbacks based on the host OS otherwise.
#
# On Linux it tries to follow the XDG directory spec. On MacOS or Windows
# it defaults based on OS convention.
hc_uri="https://github.com/mitre/hipcheck/archive/refs/heads/main.zip"
if [ "$os_name" = "linux" ]; then
    if [ -n "$XDG_BIN_HOME" ]; then
        hc_bin_default="$XDG_BIN_HOME/hipcheck"
    else
        hc_bin_default="$HOME/.local/bin"
    fi
    if [ -n "$XDG_CONFIG_HOME" ]; then
        hc_config_default="$XDG_CONFIG_HOME/hipcheck"
    else
        hc_config_default="$HOME/.config/hipcheck"
    fi
    if [ -n "$XDG_DATA_HOME" ]; then
        hc_data_default="$XDG_DATA_HOME/hipcheck"
    else
        hc_data_default="$HOME/.local/share/hipcheck"
    fi
elif [ "$os_name" = "macos" ]; then
    hc_bin_default="$HOME/.local/bin"
    hc_config_default="$HOME/Library/Application Support/hipcheck"
    hc_data_default="$HOME/Library/Application Support/hipcheck"
elif [ "$os_name" = "windows" ]; then
    hc_bin_default="$HOME/.local/bin"
    # shellcheck disable=SC2154
    if [ -n "$FOLDERID_RoamingAppData" ]; then
        hc_config_default="$FOLDERID_RoamingAppData/hipcheck"
        hc_data_default="$FOLDERID_RoamingAppData/hipcheck"
    else
        hc_config_default="$HOME/AppData/Roaming/hipcheck"
        hc_data_default="$HOME/AppData/Roaming/hipcheck"
    fi
else
    echo "error: unknown operating system" 1>&2
    exit 1
fi
hc_bin="${HC_BIN:-$hc_bin_default}"
hc_config="${HC_CONFIG:-$hc_config_default}"
hc_data="${HC_DATA:-$hc_data_default}"
hc_unzipped="$hc_bin/hipcheck-main"

# Prep Directories
#
# This makes sure the directories we need exist already, and will handle creating
# any missing parent directories as well.
if [ ! -d "$hc_bin" ]; then mkdir -p "$hc_bin"; fi
if [ ! -d "$hc_config" ]; then mkdir -p "$hc_config"; fi
if [ ! -d "$hc_data" ]; then mkdir -p "$hc_data"; fi

# Download Hipcheck
#
# This downloads the Hipcheck bundle to a `.tar.gz` file, then untars the file,
# removes the tarball, and moves into the now-untarred directory.
echo "Downloading Hipcheck..."
curl --fail --location --progress-bar --output "$hc_bin/hc.tar.gz" "$hc_uri"
cd "$hc_bin"
tar xzf "$hc_bin/hc.tar.gz"
rm "$hc_bin/hc.tar.gz"
cd "$hc_unzipped"
echo ""

# Build Hipcheck
#
# This builds Hipcheck in release mode with Cargo.
echo "Building Hipcheck..."
cargo build --release

# Copy Files
#
# This copies Hipcheck's binary to the configured bin folder, the configuration
# files to the configured config folder, and scripts to the configured scripts
# folder. Then it sets the execute bit on the binary in case it's missing for
# some strange reason, and then deletes the previously downloaded / untarred
# Hipcheck directory.
cp ".target/release/hc" "$hc_bin/hc"
cp -R "$hc_unzipped/config/." "$hc_config"
cp -R "$hc_unzipped/scripts/." "$hc_data"
chmod +x "$hc_bin/hc"
cd ..
rm -r "$hc_unzipped"

# Final Report
#
# Finally, this tells the user if they need to set anything in their relevant
# shell profile file, and what needs to be set.
echo ""
echo "'hc' was installed successfully to '$hc_bin/hc'"
case $SHELL in
    /bin/zsh | /usr/bin/zsh) shell_profile=".zshrc" ;;
    /bin/bash | /usr/bin/bash) shell_profile=".bash_profile" ;;
    *) shell_profile=".profile" ;;
esac
echo ""
echo "Manually add the following to your '\$HOME/$shell_profile' (or similar)"
echo ""
case ":$PATH:" in
    *:$hc_bin:*) hc="hc" ;;
    *)  echo "  export PATH=\"$hc_bin:\$PATH\""
        hc="$hc_bin/hc";;
esac
echo "  export HC_CONFIG=\"$hc_config\""
echo "  export HC_DATA=\"$hc_data\""
echo ""
echo "Run '$hc help' to get started"

