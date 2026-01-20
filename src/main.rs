use clap::Parser;
use kcp::{KcpConfig, KcpNoDelayConfig, KcpStream, KcpUdpStream};
use std::sync::{Arc, LazyLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use uuid::Uuid;

#[derive(Parser)]
struct Args {
    /// 运行服务端模式
    #[arg(long, default_value_t = false, group = "mode")]
    server: bool,

    /// 运行客户端模式
    #[arg(long, default_value_t = true, group = "mode")]
    client: bool,

    /// 服务端模式下的代理地址，客户端模式下的远程连接地址
    #[arg(short, long)]
    proxy_addr: String,

    /// 服务端模式下的监听地址，客户端模式下的本地监听地址
    #[arg(short, long, default_value = "0.0.0.0:25565")]
    listen_addr: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if !args.client && !args.server {
        eprintln!("Error: You should specify one mode")
    } else if args.client {
        println!("Run in client mode...");
        run_server(&args).await?;
    } else {
        println!("Run in server mode...");
        run_client(&args).await?;
    }

    Ok(())
}

async fn run_server(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let udp_socket = UdpSocket::bind(&args.listen_addr).await?;
    println!("Server UDP bound to {:?}", udp_socket.local_addr()?);
    let mut kcp_listener = KcpUdpStream::socket_listen(KCP_CONFIG.clone(), udp_socket, 5, None)?;

    println!(
        "Begin forward task: tcp://{} <-> kcp://{}",
        &args.proxy_addr, &args.listen_addr
    );

    loop {
        println!("Waiting for new client connection...");
        let income_connection = kcp_listener.accept().await?;
        let session_id = Uuid::new_v4().to_string();
        println!(
            "New connection from client {:?}, with session id {}",
            income_connection.1, session_id
        );
        let proxy_addr = args.proxy_addr.clone();
        tokio::spawn(async move {
            if let Ok(tcp_stream) = TcpStream::connect(&proxy_addr).await {
                let session_result =
                    handle_session(tcp_stream, income_connection.0, session_id.clone()).await;
                handle_session_result(session_id, session_result);
            } else {
                eprintln!(
                    "Session {}: Failed to connection to tcp endpoint({})",
                    session_id, proxy_addr
                );
            };
        });
    }
}

async fn run_client(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let tcp_listener = TcpListener::bind(&args.listen_addr).await?;
    println!("Client TCP listening on {:?}", tcp_listener.local_addr()?);
    loop {
        println!("Waiting for new connection...");
        let session_id = Uuid::new_v4().to_string();
        let (tcp_stream, _) = tcp_listener.accept().await?;
        println!(
            "New connection from {:?}, with session id {}",
            tcp_stream.peer_addr()?,
            session_id
        );
        let remote_addr = args.proxy_addr.clone();
        tokio::spawn(async move {
            if let Ok(kcp_stream) = KcpUdpStream::connect(KCP_CONFIG.clone(), &remote_addr).await {
                let session_result =
                    handle_session(tcp_stream, kcp_stream.0, session_id.clone()).await;
                handle_session_result(session_id, session_result);
            } else {
                eprintln!(
                    "Session {}: Failed to connect to kcp endpoint({})",
                    session_id, remote_addr
                );
            };
        });
    }
}

async fn handle_session(
    mut tcp_stream: TcpStream,
    mut kcp_stream: KcpStream,
    session_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tcp_to_kcp_buffer = [0; 4096];
    let mut kcp_to_tcp_buffer = [0; 4096];

    loop {
        tokio::select! {
            // TCP → KCP
            result = tcp_stream.read(&mut tcp_to_kcp_buffer) => {
                match result {
                    Ok(0) => break,
                    Ok(n) => {
                        kcp_stream.write_all(&tcp_to_kcp_buffer[..n]).await?;
                        kcp_stream.flush().await?;
                    },
                    Err(e) => {
                        eprintln!("Session {}: TCP read error: {}", session_id, e);
                        break;
                    }
                }
            }

            // KCP → TCP
            result = kcp_stream.read(&mut kcp_to_tcp_buffer) => {
                match result {
                    Ok(0) => break,
                    Ok(n) => {
                        tcp_stream.write_all(&kcp_to_tcp_buffer[..n]).await?;
                        tcp_stream.flush().await?;
                    },
                    Err(e) => {
                        eprintln!("Sessoin {}: KCP read error: {}", session_id, e);
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

fn handle_session_result(session_id: String, result: Result<(), Box<dyn std::error::Error>>) {
    match result {
        Err(e) => {
            eprintln!(
                "Session {}: occurred an error, {}",
                session_id,
                e.to_string()
            )
        }
        Ok(()) => println!("Session {}: End of life.", session_id),
    }
}

static KCP_CONFIG: LazyLock<Arc<KcpConfig>> = LazyLock::new(|| {
    Arc::new(KcpConfig {
        mtu: 1400,
        stream: true,
        nodelay: KcpNoDelayConfig {
            nodelay: true,
            interval: 40,
            resend: 2,
            nc: true,
        },
        rcv_wnd: 1024,
        snd_wnd: 1024,
        ..Default::default()
    })
});
