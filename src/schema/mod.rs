mod error_capnp;
mod host_capnp;
mod screen_capnp;
mod stream_capnp;

pub mod error {
    #[doc(inline)]
    pub use super::error_capnp::*;
}

pub mod host {
    #[doc(inline)]
    pub use super::host_capnp::*;
}

pub mod screen {
    #[doc(inline)]
    pub use super::screen_capnp::*;
}

pub mod stream {
    #[doc(inline)]
    pub use super::stream_capnp::*;
}
