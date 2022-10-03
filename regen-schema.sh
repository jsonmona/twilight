#!/usr/bin/env sh

# This script should be run every time schema is modified
# When modifying this file, please keep the PowerShell version in sync

# Steps to prepare:
# 1. Install capnp official binary
# 2. Install rust plugin by running `cargo install capnpc`
# 3. Run this script

set -eux

OUT_DIR='src'

capnp compile -orust:$OUT_DIR schema/*.capnp
