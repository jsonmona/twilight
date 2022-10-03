@0xc7abb4188a2ffcd3;

using Rust = import "rust.capnp";
$Rust.parentModule("schema");

using import "error.capnp".Result;
using import "stream.capnp".Stream;

interface Screen {
    open @0 () -> (streamId :Result(Stream));
}

struct DisplayFrame {
    keyframe @0 :Bool;      # True if keyframe (IDR frame)
    data @1 :Data;          # Data of the encoded frame
}
