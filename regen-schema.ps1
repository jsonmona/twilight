# Please look at .sh version for help

$OUT_DIR="src"

$files = (Get-ChildItem .\schema\*.capnp | Select-Object -Expand FullName | Resolve-Path -Relative)

capnp compile -orust:$OUT_DIR $files
