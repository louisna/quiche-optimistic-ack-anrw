// Copyright (C) 2019, Cloudflare, Inc.
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
//     * Redistributions of source code must retain the above copyright notice,
//       this list of conditions and the following disclaimer.
//
//     * Redistributions in binary form must reproduce the above copyright
//       notice, this list of conditions and the following disclaimer in the
//       documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
// IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO,
// THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
// PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
// CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,
// EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
// PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
// LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
// NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
// SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

#[macro_use]
extern crate log;

use clap::Parser;
use quiche_apps::common::make_qlog_writer;
use quiche_apps::oack::OpportunistAck;
use std::net::ToSocketAddrs;

use quiche::h3::NameValue;

use ring::rand::*;

const MAX_DATAGRAM_SIZE: usize = 1350;

#[derive(Parser)]
struct Args {
    /// URL of the server.
    url: url::Url,

    /// Whether the client injects opportunist acknowledgments.
    #[clap(long = "oack", value_parser)]
    do_oack: bool,

    /// OACK QLOG file path.
    /// If not provided, uses the "max received pn" approach instead.
    #[clap(long = "qlog")]
    qlog_path: Option<String>,

    /// Whether to use HTTP/0.9 for QUIC interop instead of HTTP/3.
    #[clap(long = "hq-interop")]
    hq_interop: bool,

    /// OACK packet number shift.
    #[clap(long = "shift-pn")]
    shift_pn: Option<u64>,

    /// OACK packet number lag shift.
    #[clap(long = "lag-shift")]
    lag_shift: Option<u64>,
}

fn main() {
    env_logger::builder().format_timestamp_micros().init();

    let mut buf = [0; 65535];
    let mut out = [0; MAX_DATAGRAM_SIZE];

    let args = Args::parse();

    let url = args.url;

    // Setup the event loop.
    let mut poll = mio::Poll::new().unwrap();
    let mut events = mio::Events::with_capacity(1024);

    // Resolve server address.
    let peer_addr = url.to_socket_addrs().unwrap().next().unwrap();

    // Bind to INADDR_ANY or IN6ADDR_ANY depending on the IP family of the
    // server address. This is needed on macOS and BSD variants that don't
    // support binding to IN6ADDR_ANY for both v4 and v6.
    let bind_addr = match peer_addr {
        std::net::SocketAddr::V4(_) => "0.0.0.0:0",
        std::net::SocketAddr::V6(_) => "[::]:0",
    };

    // Create the UDP socket backing the QUIC connection, and register it with
    // the event loop.
    let mut socket =
        mio::net::UdpSocket::bind(bind_addr.parse().unwrap()).unwrap();
    poll.registry()
        .register(&mut socket, mio::Token(0), mio::Interest::READABLE)
        .unwrap();

    // Create the configuration for the QUIC connection.
    let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION).unwrap();

    // *CAUTION*: this should not be set to `false` in production!!!
    config.verify_peer(false);

    config
        .set_application_protos(quiche::h3::APPLICATION_PROTOCOL)
        .unwrap();

    config.set_max_idle_timeout(5000);
    if args.hq_interop {
        config.set_application_protos(&[b"hq-interop"]).unwrap();
    }
    config.set_max_recv_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_max_send_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_initial_max_data(10_000_000);
    config.set_initial_max_stream_data_bidi_local(1_000_000);
    config.set_initial_max_stream_data_bidi_remote(1_000_000);
    config.set_initial_max_stream_data_uni(1_000_000);
    // config.set_initial_max_data(100_000_000_000);
    // config.set_initial_max_stream_data_bidi_local(100_000_000_000);
    // config.set_initial_max_stream_data_bidi_remote(100_000_000_000);
    // config.set_initial_max_stream_data_uni(100_000_000_000);
    config.set_initial_max_streams_bidi(100);
    config.set_initial_max_streams_uni(100);
    config.set_disable_active_migration(true);

    let mut keylog: Option<std::fs::File> = None;
    if args.hq_interop {
        // let file = std::fs::OpenOptions::new()
        //     .create(true)
        //     .append(true)
        //     .open("/logs/keylog.txt")
        //     .unwrap();

        // keylog = Some(file);

        // config.log_keys();
    }

    // let mut http3_conn = None;

    // Generate a random source connection ID for the connection.
    let mut scid = [0; quiche::MAX_CONN_ID_LEN];
    SystemRandom::new().fill(&mut scid[..]).unwrap();

    let scid = quiche::ConnectionId::from_ref(&scid);

    // Get local address.
    let local_addr = socket.local_addr().unwrap();

    // Create a QUIC connection and initiate handshake.
    let domain = url.domain().or(Some("server4"));
    let mut conn =
        quiche::connect(domain, &scid, local_addr, peer_addr, &mut config)
            .unwrap();

    if let Some(keylog) = &mut keylog {
        if let Ok(keylog) = keylog.try_clone() {
            conn.set_keylog(Box::new(keylog));
        }
    }

    // Only bother with qlog if the user specified it.
    #[cfg(feature = "qlog")]
    {
        if let Some(dir) = std::env::var_os("QLOGDIR") {
            let id = format!("{scid:?}");
            let writer = make_qlog_writer(&dir, "client", "0");

            conn.set_qlog(
                std::boxed::Box::new(writer),
                "quiche-client qlog".to_string(),
                format!("{} id={}", "quiche-client qlog", id),
            );
        }
    }

    info!(
        "connecting to {:} from {:} with scid {}",
        peer_addr,
        socket.local_addr().unwrap(),
        hex_dump(&scid)
    );

    // Enable oportunist acknowledgments.
    let mut oack = if args.do_oack {
        conn.enable_oack(10);
        Some(
            OpportunistAck::new(
                args.qlog_path.as_deref(),
                args.shift_pn,
                args.lag_shift,
            )
            .unwrap(),
        )
    } else {
        None
    };

    // Whether we did increase the flow control rate.
    let mut did_increase_max_data = false;

    let mut h3_stream_id = Some(4);

    let (write, send_info) = conn.send(&mut out).expect("initial send failed");

    while let Err(e) = socket.send_to(&out[..write], send_info.to) {
        if e.kind() == std::io::ErrorKind::WouldBlock {
            continue;
        }

        panic!("send() failed: {:?}", e);
    }

    let h3_config = quiche::h3::Config::new().unwrap();

    // Prepare request.
    let mut path = String::from(url.path());

    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }

    let req = vec![
        quiche::h3::Header::new(b":method", b"GET"),
        quiche::h3::Header::new(b":scheme", url.scheme().as_bytes()),
        quiche::h3::Header::new(
            b":authority",
            url.host_str().unwrap().as_bytes(),
        ),
        quiche::h3::Header::new(b":path", path.as_bytes()),
        quiche::h3::Header::new(b"user-agent", b"quiche"),
    ];

    let req_start = std::time::Instant::now();

    let mut req_sent = false;
    let mut http3_conn = None;

    loop {
        let timeout =
            [conn.timeout(), oack.as_mut().map(|o| o.timeout()).flatten()]
                .iter()
                .flatten()
                .min()
                .copied();
        poll.poll(&mut events, timeout).unwrap();

        // Read incoming UDP packets from the socket and feed them to quiche,
        // until there are no more packets to read.
        'read: loop {
            // If the event loop reported no events, it means that the timeout
            // has expired, so handle it without attempting to read packets. We
            // will then proceed with the send loop.
            if events.is_empty() {
                conn.on_timeout();

                break 'read;
            }

            if let Some(oack) = oack.as_mut() {
                oack.on_timeout(&mut conn, h3_stream_id.unwrap()).unwrap();
            }

            let (len, from) = match socket.recv_from(&mut buf) {
                Ok(v) => v,

                Err(e) => {
                    // There are no more UDP packets to read, so end the read
                    // loop.
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        debug!("recv() would block");
                        break 'read;
                    }

                    panic!("recv() failed: {:?}", e);
                },
            };

            let recv_info = quiche::RecvInfo {
                to: local_addr,
                from,
            };

            // Process potentially coalesced packets.
            let _read = match conn.recv(&mut buf[..len], recv_info) {
                Ok(v) => v,

                Err(e) => {
                    error!("recv failed: {:?}", e);
                    continue 'read;
                },
            };
        }

        if let Some(oack) = oack.as_mut() {
            let max_pn = conn.oack_get_max_pn();
            oack.on_new_max_recv_pn(max_pn);
        }

        if conn.is_established() {
            if let Some(oack) = oack.as_mut() {
                if !oack.is_active() {
                    oack.set_active(true);
                }
            }
        }

        if conn.is_closed() {
            info!("connection closed, {:?}", conn.stats());
            break;
        }

        // Create the HTTP/3 or HTTP/0.9 connection.
        if args.hq_interop {
            if conn.is_established() && !req_sent {
                info!("sending HTTP request for {}", url.path());

                let req = format!("GET {}\r\n", url.path());
                conn.stream_send(4, req.as_bytes(), true).unwrap();

                req_sent = true;
                h3_stream_id = Some(4);
            }
        } else {
            // Create a new HTTP/3 connection once the QUIC connection is
            // established.
            if conn.is_established() && http3_conn.is_none() {
                http3_conn = Some(
                    quiche::h3::Connection::with_transport(&mut conn, &h3_config)
                    .expect("Unable to create HTTP/3 connection, check the server's uni stream limit and window size"),
                );
            }

            // Send HTTP requests once the QUIC connection is established, and
            // until all requests have been sent.
            if let Some(h3_conn) = &mut http3_conn {
                if !req_sent {
                    info!("sending HTTP request {:?}", req);

                    h3_stream_id = Some(
                        h3_conn.send_request(&mut conn, &req, true).unwrap(),
                    );

                    req_sent = true;
                }
            }

            if let Some(http3_conn) = &mut http3_conn {
                // Process HTTP/3 events.
                loop {
                    match http3_conn.poll(&mut conn) {
                        Ok((
                            stream_id,
                            quiche::h3::Event::Headers { list, .. },
                        )) => {
                            info!(
                                "got response headers {:?} on stream id {}",
                                hdrs_to_strings(&list),
                                stream_id
                            );
                        },

                        Ok((stream_id, quiche::h3::Event::Data)) => {
                            if !args.do_oack {
                                while let Ok(_read) = http3_conn
                                    .recv_body(&mut conn, stream_id, &mut buf)
                                {
                                    // println!(
                                    //     "got {} bytes of response data on
                                    // stream
                                    // {}",
                                    //     read, stream_id
                                    // );

                                    // print!("{}", unsafe {
                                    //     std::str::from_utf8_unchecked(&buf[..
                                    // read]) });
                                }
                            }
                        },

                        Ok((_stream_id, quiche::h3::Event::Finished)) => {
                            info!(
                                "response received in {:?}, closing...",
                                req_start.elapsed()
                            );

                            conn.close(true, 0x100, b"kthxbye").unwrap();
                        },

                        Ok((_stream_id, quiche::h3::Event::Reset(e))) => {
                            error!(
                                "request was reset by peer with {}, closing...",
                                e
                            );

                            conn.close(true, 0x100, b"kthxbye").unwrap();
                        },

                        Ok((_, quiche::h3::Event::PriorityUpdate)) =>
                            unreachable!(),

                        Ok((goaway_id, quiche::h3::Event::GoAway)) => {
                            info!("GOAWAY id={}", goaway_id);
                        },

                        Err(quiche::h3::Error::Done) => {
                            break;
                        },

                        Err(e) => {
                            error!("HTTP/3 processing failed: {:?}", e);

                            break;
                        },
                    }
                }
            }
        }

        if let Some(sid) = h3_stream_id {
            if conn.stream_readable(sid) {
                if oack.is_some() {
                    if !did_increase_max_data {
                        did_increase_max_data = true;

                        // Send new MAX_DATA and MAX_STREAM_DATA frames to
                        // increase the flow control
                        // limits.
                        conn.oack_new_max_data(h3_stream_id.unwrap());
                    }
                }
            }
        }

        for stream_id in conn.readable() {
            while conn.stream_readable(stream_id) {
                let (_n, _) = conn.stream_recv(stream_id, &mut out).unwrap();
                // println!("RESPONSE: {:?}",
                // String::from_utf8((&out[..n]).to_vec()));
            }
        }

        // Generate outgoing QUIC packets and send them on the UDP socket, until
        // quiche reports that there are no more packets to be sent.
        loop {
            let (write, send_info) = match conn.send(&mut out) {
                Ok(v) => v,

                Err(quiche::Error::Done) => {
                    break;
                },

                Err(e) => {
                    error!("send failed: {:?}", e);

                    conn.close(false, 0x1, b"fail").ok();
                    break;
                },
            };

            if let Err(e) = socket.send_to(&out[..write], send_info.to) {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    break;
                }

                panic!("send() failed: {:?}", e);
            }
        }

        if conn.is_closed() {
            println!("connection closed, {:?}", conn.stats());
            break;
        }
    }
}

fn hex_dump(buf: &[u8]) -> String {
    let vec: Vec<String> = buf.iter().map(|b| format!("{b:02x}")).collect();

    vec.join("")
}

pub fn hdrs_to_strings(hdrs: &[quiche::h3::Header]) -> Vec<(String, String)> {
    hdrs.iter()
        .map(|h| {
            let name = String::from_utf8_lossy(h.name()).to_string();
            let value = String::from_utf8_lossy(h.value()).to_string();

            (name, value)
        })
        .collect()
}
