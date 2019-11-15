Net-Tiger
=========

This is a rewrite of net-cat for the modern internet. The goal is to provide a
single, efficient command line utility to support connecting the network to
your terminal. The key thing that sets this apart from the traditional net-cat
is the *protocol support*.

# Protocols
Net-tiger, for now, has goals to support the following protocols.

- [ ] Transmission Control Protocol (`tcp://`)
- [ ] Transport Layer Security (`tls://`)
- [ ] Web Sockets (`ws://` or `wss://`)
- [ ] Quic (`quic://`)

It's currently early days, but that's where we are headed. If there's a
protocol you think should be on this list that isn't create an issue.

# Usage
It's pretty simple.

```
nt [FLAGS] <url>
```

The only accepted flag at the moment is verbosity (`-v` or `-vvvvv` or
somewhere in between). The URL is where you will connect to, and you simply use
one of the URL schemes from the protocols section. When I get to server
support, this will change slightly.

# Efficiency
Net-cat is a very simple program - it maintains an open connection to the
network, and an open connection to stdin/stdout, and it connects them. The
net-cat implementations I've looked at do this with very little resource
consumption: only one thread, minimum memory overhead. While this is a little
more complex than opening a socket and calling poll(2), I'm hoping that
net-tiger can achieve the same.

To try to minimise general CPU / memory overhead, this is written in rust. (Rust
has some other advantages for the project, like being a language I enjoy
writing in and a language that supports easy distribution of compiled
binaries) To keep things on a single thread, net-tiger uses `async`/`await`. I
couldn't find an executor runtime that used only a single thread*, and I've
wanted to for quite some time now, so I'm writing a custom executor for use in
net-tiger that integrates with the `std::task` and `std::future` modules. IO
multiplexing is done with [mio](https://github.com/tokio-rs/mio).

*while Tokio does have a single threaded executor, it doesn't play too nicely
with stdio. In a past version it wouldn't support it, and now it spins up a
second thread for reading from stdin. While this probably isn't too bad, as far
as efficiency is concerned, I already wanted to write my own executor anyway.
As a bonus, we will get exactly 1 thread in use by the application.
