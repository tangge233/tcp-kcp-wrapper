use clap::Parser;
use kcp::{KcpConfig, KcpNoDelayConfig, KcpStream, KcpUdpStream};
use std::sync::{Arc, LazyLock};
use tokio::io::{self, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use uuid::Uuid;

#[derive(Parser)]
struct Args {
    /// 运行服务端模式
    #[arg(short, long, default_value_t = false, group = "mode")]
    server: bool,

    /// 运行客户端模式
    #[arg(short, long, default_value_t = true, group = "mode")]
    client: bool,

    /// 服务端模式下的代理地址，客户端模式下的远程连接地址
    #[arg(long)]
    proxy_addr: String,

    /// 服务端模式下的监听地址，客户端模式下的本地监听地址
    #[arg(long, default_value = "0.0.0.0:25565")]
    listen_addr: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if !args.client && !args.server {
        eprintln!("Error: You should specify one mode")
    } else if args.server {
        println!("Run in server mode...");
        run_server(&args).await?;
    } else {
        println!("Run in client mode...");
        run_client(&args).await?;
    }

    Ok(())
}

async fn run_server(args: &Args) -> anyhow::Result<()> {
    let udp_socket = UdpSocket::bind(&args.listen_addr).await?;
    println!("Server UDP bound to {:?}", udp_socket.local_addr()?);
    let mut kcp_listener = KcpUdpStream::socket_listen(KCP_CONFIG.clone(), udp_socket, 5, None)?;

    println!(
        "Begin forward task: tcp://{} <-> kcp://{}",
        &args.proxy_addr, &args.listen_addr
    );

    loop {
        println!("Waiting for new client connection...");
        let (income_stream, income_addr) = kcp_listener.accept().await?;
        let session_id = Uuid::new_v4().to_string();
        println!("New connection from client {income_addr}, with session id {session_id}",);
        let proxy_addr = args.proxy_addr.clone();
        tokio::spawn(async move {
            if let Ok(tcp_stream) = TcpStream::connect(&proxy_addr).await {
                let session_result =
                    handle_session(tcp_stream, income_stream, session_id.clone()).await;
                handle_session_result(session_id, session_result);
            } else {
                eprintln!(
                    "Session {session_id}: Failed to connection to tcp endpoint({proxy_addr})"
                );
            };
        });
    }
}

async fn run_client(args: &Args) -> anyhow::Result<()> {
    let tcp_listener = TcpListener::bind(&args.listen_addr).await?;
    println!("Client TCP listening on {:?}", tcp_listener.local_addr()?);
    loop {
        println!("Waiting for new connection...");
        let session_id = Uuid::new_v4().to_string();
        let (tcp_stream, _) = tcp_listener.accept().await?;
        println!(
            "New connection from {:?}, with session id {session_id}",
            tcp_stream.peer_addr()?
        );
        let remote_addr = args.proxy_addr.clone();
        tokio::spawn(async move {
            if let Ok(kcp_stream) = KcpUdpStream::connect(KCP_CONFIG.clone(), &remote_addr).await {
                let session_result =
                    handle_session(tcp_stream, kcp_stream.0, session_id.clone()).await;
                handle_session_result(session_id, session_result);
            } else {
                eprintln!("Session {session_id}: Failed to connect to kcp endpoint({remote_addr})");
            };
        });
    }
}

async fn handle_session(
    mut tcp_stream: TcpStream,
    mut kcp_stream: KcpStream,
    session_id: String,
) -> anyhow::Result<()> {
    let (writed, readed) = io::copy_bidirectional(&mut tcp_stream, &mut kcp_stream).await?;

    if let Err(e) = tcp_stream.shutdown().await {
        eprintln!("Session {session_id}: TCP shutdown error (ignored): {e}");
    }
    if let Err(e) = kcp_stream.shutdown().await {
        eprintln!("Session {session_id}: KCP shutdown error (ignored): {e}");
    }

    println!("Session {session_id} closed, wtited {writed} bytes, readed {readed} bytes");

    Ok(())
}

fn handle_session_result(session_id: String, result: anyhow::Result<()>) {
    match result {
        Err(e) => {
            eprintln!("Session {session_id}: occurred an error, {e}")
        }
        Ok(()) => println!("Session {session_id}: End of life."),
    }
}

static KCP_CONFIG: LazyLock<Arc<KcpConfig>> = LazyLock::new(|| {
    Arc::new(KcpConfig {
        mtu: 1380,
        stream: true,
        nodelay: KcpNoDelayConfig {
            nodelay: true,
            interval: 60,
            resend: 3,
            nc: true,
        },
        rcv_wnd: 1024,
        snd_wnd: 1024,
        ..Default::default()
    })
});
