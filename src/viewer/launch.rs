use crate::client::{ClientLaunchArgs, TwilightClient};
use crate::viewer::viewer_app::ViewerApp;

use std::rc::Rc;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tokio::task::LocalSet;

pub fn launch(rt: Handle, args: ClientLaunchArgs) -> ! {
    let mut viewer_app = ViewerApp::build(rt.clone());
    let proxy = viewer_app.create_proxy();
    let (quit_tx, quit_rx) = oneshot::channel();

    let worker = std::thread::spawn(move || {
        let local = LocalSet::new();

        let callback = move |event| {
            proxy.send_event(event).unwrap();
        };

        tokio::pin!(quit_rx);
        tokio::pin!(local);

        rt.block_on(async move {
            let _guard = local.enter();
            let client = TwilightClient::new(Rc::new(callback), args);

            tokio::select! {
                biased;
                _ = &mut quit_rx => {},
                _ = &mut local => {},
            }

            client.close();
            local.await;
        });
    });

    viewer_app.set_on_exit(Box::new(|| {
        let _ = quit_tx.send(true);
        worker.join().unwrap();
        Ok(())
    }));

    viewer_app.launch();
}
