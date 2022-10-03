@0xda0b738bf780059f;

using Rust = import "rust.capnp";
$Rust.parentModule("schema");

interface Stream {
    id @0 () -> (id :UInt32);
}
