use tokio::task::LocalSet;

#[tokio::main]
async fn main() {
    env_logger::init();

    let main_thread = LocalSet::new();

    main_thread.run_until(twilight::viewer::launch()).await;
    main_thread.await;
}
