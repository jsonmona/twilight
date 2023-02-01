#!/bin/sh

# This script should be run every time schema is modified
# When modifying this file, please keep the PowerShell version in sync

# Steps to prepare:
# 1. Install flatc compiler (possibly using prebuilt one)
# 2. Run this script

set -eux

OUT_DIR='./src/schema'

flatc -o "$OUT_DIR" --rust schema/*.fbs
