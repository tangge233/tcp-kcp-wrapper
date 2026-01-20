use clap::Parser;
use kcp::{KcpConfig, KcpNoDelayConfig, KcpStream, KcpUdpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::{Arc, LazyLock};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

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

    if args.server {
        run_server(&args).await?;
    } else if args.client {
        run_client(&args).await?;
    } else {
        eprintln!("Error: must specify --server or --client");
        std::process::exit(1);
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
        let income_connection = kcp_listener.accept().await?;
        println!("New connection from {:?}", income_connection.1);
        let proxy_addr = args.proxy_addr.clone();
        tokio::spawn(async move {
            let _ = handle_server_session(proxy_addr, income_connection.0).await;
        });
    }
}

async fn handle_server_session(
    tcp_addr: String,
    mut kcp_stream: KcpStream,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tcp_session = TcpStream::connect(tcp_addr).await?;
    let mut tcp_to_kcp_buffer = [0; 4096];
    let mut kcp_to_tcp_buffer = [0; 4096];

    loop {
        tokio::select! {
            // TCP → KCP
            result = tcp_session.read(&mut tcp_to_kcp_buffer) => {
                match result {
                    Ok(0) => break,  // TCP连接关闭
                    Ok(n) => {
                        kcp_stream.write_all(&tcp_to_kcp_buffer[..n]).await?;
                        kcp_stream.flush().await?;
                    }
                    Err(e) => {
                        eprintln!("TCP read error: {}", e);
                        break;
                    }
                }
            }

            // KCP → TCP
            result = kcp_stream.read(&mut kcp_to_tcp_buffer) => {
                match result {
                    Ok(0) => break,
                    Ok(n) => {
                        tcp_session.write_all(&kcp_to_tcp_buffer[..n]).await?;
                        tcp_session.flush().await?;
                    }
                    Err(e) => {
                        eprintln!("KCP read error: {}", e);
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn run_client(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let tcp_listener = TcpListener::bind(&args.listen_addr).await?;
    println!("Client TCP listening on {:?}", tcp_listener.local_addr()?);
    loop {
        let (tcp_stream, _) = tcp_listener.accept().await?;
        println!("New connection from {:?}", tcp_stream.peer_addr()?);
        let kcp_addr = args.proxy_addr.clone();
        tokio::spawn(async move {
            let _ = handle_client_session(tcp_stream, kcp_addr).await;
        });
    }

    Ok(())
}

async fn handle_client_session(
    mut tcp_stream: TcpStream,
    kcp_addr: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut kcp_connection = KcpUdpStream::connect(KCP_CONFIG.clone(), kcp_addr).await?;
    let mut kcp_stream = kcp_connection.0;
    let mut tcp_to_kcp_buffer = [0; 4096];
    let mut kcp_to_tcp_buffer = [0; 4096];

    loop {
        tokio::select! {
            // TCP → KCP
            result = tcp_stream.read(&mut tcp_to_kcp_buffer) => {
                match result {
                    Ok(0) => break,  // TCP连接关闭
                    Ok(n) => {
                        kcp_stream.write_all(&tcp_to_kcp_buffer[..n]).await?;
                        kcp_stream.flush().await?;
                    },
                    Err(e) => {
                        eprintln!("TCP read error: {}", e);
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
                        eprintln!("KCP read error: {}", e);
                        break;
                    }
                }
            }
        }
    }


    Ok(())
}

static KCP_CONFIG: LazyLock<Arc<KcpConfig>> = LazyLock::new(||{
    Arc::new(KcpConfig {
        mtu: 1400,
        stream: true,
        nodelay: KcpNoDelayConfig {
            nodelay: true,
            interval: 40,
            resend: 2,
            nc: true,
        },
        ..Default::default()
    })
});
