use crate::client::native_server_connection::NativeServerConnection;
use crate::client::TwilightClient;
use crate::viewer::viewer_app::ViewerApp;
use std::net::IpAddr;
use std::rc::Rc;
use tokio::runtime::Handle;
use tokio::task::LocalSet;

pub fn launch(rt: Handle, ip: IpAddr, port: u16) -> ! {
    let viewer_app = ViewerApp::new(rt.clone());
    let proxy = viewer_app.create_proxy();

    std::thread::spawn(move || {
        let local = LocalSet::new();

        let callback = move |event| {
            proxy.send_event(event).unwrap();
        };

        rt.block_on(async move {
            let _guard = local.enter();
            let _client = TwilightClient::new(Rc::new(callback), async move {
                NativeServerConnection::new(ip, port).await
            });
            local.await;
        });
    });

    pollster::block_on(viewer_app.launch())
}
