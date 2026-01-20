# TCP -> 📦 -> KCP

## 这个能干嘛

就是简单地用 KCP 包裹 TCP 数据进行传输，在 TCP 连接受到限制的情况（比如 Minecraft 联机）中可以有效减低延迟。~~UDP 被阻断另当别论了（笑~~

## How to use

使用 --help 可以查看使用方法

```
Usage: tcp-kcp-wrapper.exe [OPTIONS] --proxy-addr <PROXY_ADDR>

Options:
      --server                     运行服务端模式
      --client                     运行客户端模式
  -p, --proxy-addr <PROXY_ADDR>    服务端模式下的代理地址，客户端模式下的远程连接地址
  -l, --listen-addr <LISTEN_ADDR>  服务端模式下的监听地址，客户端模式下的本地监听地址 [default: 0.0.0.0:25565]
  -h, --help                       Print help
```

对于客户端直接使用
```
./tcp-kcp-wrapper --proxy-addr <远程地址>
```
就只可以直接在本地 25565 端口开一个入口端口，连接即可访问远程服务

比如

```
./tcp-kcp-wrapper --proxy-addr 1.1.1.1:25565
```

对于服务端，可以这样使用

```
./tcp-kcp-wrapper --server --proxy-addr <代理地址> --listen-addr <监听地址>
```

比如

```
./tcp-kcp-wrapper --server --proxy-addr 127.0.0.1:25565 --listen-addr 0.0.0.0:25565
```
由于监听用的 UDP，甚至可以直接使用同端口的地址。

## LICENSE

本项目以 MIT 许可证开源
