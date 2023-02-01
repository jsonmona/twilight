fn main() {
    twilight::platform::win32::init_dpi();
    env_logger::init();

    twilight::server::serve().expect("unable to launch server");
}
