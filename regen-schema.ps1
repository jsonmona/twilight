# Please look at .sh version for help

$OUT_DIR="src\schema"

$files = (Get-ChildItem .\schema\*.fbs | Select-Object -Expand FullName | Resolve-Path -Relative)

flatc -o "$OUT_DIR" --rust $files
