fn main() {
    twilight::platform::win32::init_dpi();
    env_logger::init();

    std::thread::spawn(|| {
        twilight::server::serve().unwrap();
    });

    twilight::viewer::launch();
}
