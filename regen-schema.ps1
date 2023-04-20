# Please look at .sh version for help

$OUT_DIR="src\schema"

flatc -o "$OUT_DIR" --gen-all --rust .\schema\schema.fbs
