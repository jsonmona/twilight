@0x8a8fe9fbfc63f210;

using import "screen.capnp".Screen;

interface Host {
    listScreen @0 () -> (displays :List(Screen));
}
