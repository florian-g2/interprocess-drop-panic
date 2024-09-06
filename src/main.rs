use anyhow::Error;
use std::time::Duration;

use interprocess::local_socket::tokio::SendHalf;
use interprocess::local_socket::traits::tokio::{Listener, Stream as StreamTrait};
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, ToNsName};
use tokio::io::AsyncWriteExt;
use tokio::net::windows::named_pipe::ClientOptions;
use tokio::runtime::Builder;
use tokio::task;
use tokio::task::yield_now;
use tokio::time::sleep;

async fn open_server() -> Result<(), Error> {
    let socket_name = "interprocess-drop-panic".to_ns_name::<GenericNamespaced>()?;
    let opts = ListenerOptions::new().name(socket_name);

    let listener = match opts.create_tokio() {
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            return Err(e.into());
        }
        x => x?,
    };

    // accept connection
    let connection = listener.accept().await?;
    let (_receiver, sender) = connection.split();

    // move sender away into the tokio runtime
    task::spawn(send_loop(sender));

    // this task is dropped, no problem so far
    Ok(())
}

async fn send_loop(mut ipc_sender: SendHalf) -> Result<(), Error> {
    ipc_sender.write_all(b"Hello, world!").await?;
    sleep(Duration::from_secs(1000)).await;
    Ok(())
}

fn main() -> Result<(), Error> {
    Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?
        .block_on(async {
            // open server
            task::spawn(open_server());

            // yield so that the server has a chance to start
            yield_now().await;

            // connect to server
            let _client = ClientOptions::new().open(r"\\.\pipe\interprocess-drop-panic")?;

            // let the server accept the connection
            yield_now().await;
            // now, the open_server task is completed and the receiver is dropped.
            
            // let the sender write "Hello, world!"
            yield_now().await;
            // now, just the sender future is alive and owned by the tokio runtime.

            // returning now will close the tokio runtime, and the sender future will be dropped.
            // when the sender is dropped, it tries to use the tokio runtime for the limbo, but that is now dead.
            // this will cause the undesired panic.
            Ok(())
        })
}
