# Please look at shell script version for help

$OUT_DIR="src\schema"

capnp compile -orust:$OUT_DIR --src-prefix=schema (Get-ChildItem .\schema\*.capnp | Select-Object -Expand FullName)
