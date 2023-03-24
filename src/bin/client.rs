use tokio::task::LocalSet;
use twilight::util::NonSend;

#[tokio::main]
async fn main() {
    env_logger::init();

    let main_thread = NonSend::new();
    let local = LocalSet::new();

    local.run_until(twilight::viewer::launch(main_thread)).await;
    local.await;
}
