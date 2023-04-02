use crate::client::native_server_connection::NativeServerConnection;
use crate::client::TwilightClient;
use crate::viewer::viewer_app::ViewerApp;
use std::net::IpAddr;
use std::rc::Rc;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tokio::task::LocalSet;

pub fn launch(rt: Handle, ip: IpAddr, port: u16) -> ! {
    let mut viewer_app = ViewerApp::new(rt.clone());
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
            let client = TwilightClient::new(Rc::new(callback), async move {
                NativeServerConnection::new(ip, port).await
            });

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

    pollster::block_on(viewer_app.launch())
}
