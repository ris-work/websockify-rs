pub use warp_websockify::WebsockifyError;

use clap::{crate_authors, crate_version, App, Arg};
use log::info;
use rust_embed::RustEmbed;
use std::env;
use std::net::ToSocketAddrs;
#[cfg(unix)]
use tokio::net::UnixListener;
#[cfg(unix)]
use tokio_stream::wrappers::UnixListenerStream;
use warp::{http::Uri, Filter};

#[derive(RustEmbed)]
#[folder = "noVNC"]
struct NoVnc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::new("WebSockify-rs")
        .version(crate_version!())
        .author(crate_authors!())
        .about("Convert TCP/Unix doamin socket connection to WebSocket")
        .arg(
            Arg::with_name("upstream")
                .index(1)
                .help("Upstream host:port")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("listen")
                .index(2)
                .help("Listen host:port")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("prefix")
                .short("p")
                .long("prefix")
                .takes_value(true)
                .help("server prefix"),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .multiple(true),
        );
    let app = if cfg!(unix) {
        app.arg(
            Arg::with_name("upstream-unix")
                .short("u")
                .long("upstream-unix")
                .help("Upstream is unix domain socket"),
        )
        .arg(
            Arg::with_name("listen-unix")
                .short("l")
                .long("listen-unix")
                .help("Listen path is unix domain socket"),
        )
    } else {
        app
    };
    let matches = app.get_matches();

    match matches.occurrences_of("verbose") {
        1 => env::set_var("RUST_LOG", "info"),
        2 => env::set_var("RUST_LOG", "debug"),
        3 => env::set_var("RUST_LOG", "trace"),
        _ => {
            if env::var("RUST_LOG").is_err() {
                env::set_var("RUST_LOG", "warn")
            }
        }
    }
    pretty_env_logger::init();

    let upstream = if matches.is_present("upstream-unix") {
        #[cfg(unix)]
        {
            warp_websockify::Destination::unix(matches.value_of("upstream").unwrap())
        }
        #[cfg(not(unix))]
        {
            unimplemented!()
        }
    } else {
        warp_websockify::Destination::tcp(matches.value_of("upstream").unwrap()).unwrap()
    };

    // Create a websocket filter without any path restrictions
    let ws = warp_websockify::websockify(upstream);

    // Read the prefix argument, even though we're not using it for routing now.
    let _prefix = matches.value_of("prefix").unwrap_or("").to_string();

    // Serve the websocket filter on ANY URL.
    let server = warp::any().and(ws.with(warp::log("http"))).boxed();

    if matches.is_present("listen-unix") {
        #[cfg(unix)]
        {
            println!(
                "Websocket server on unix://{}",
                matches.value_of("listen").unwrap()
            );
            let listener = UnixListener::bind(matches.value_of("listen").unwrap())?;
            let incoming = UnixListenerStream::new(listener);
            warp::serve(server).run_incoming(incoming).await;
        }
        #[cfg(not(unix))]
        {
            unimplemented!()
        }
    } else {
        println!(
            "Websocket server on http://{}",
            matches.value_of("listen").unwrap()
        );
        let listen = matches.value_of("listen").unwrap().to_socket_addrs()?;

        let binded: Vec<_> = listen
            .map(|x| {
                let binded = warp::serve(server.clone()).bind(x);
                info!("binded: {}", x);
                tokio::spawn(binded)
            })
            .collect();
        for one in binded {
            one.await?;
        }
    }

    Ok(())
}
