// Copyright (C) 2018-2019, Cloudflare, Inc.
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

//! 🥧 Savoury implementation of the QUIC transport protocol and HTTP/3.
//!
//! [quiche] is an implementation of the QUIC transport protocol and HTTP/3 as
//! specified by the [IETF]. It provides a low level API for processing QUIC
//! packets and handling connection state. The application is responsible for
//! providing I/O (e.g. sockets handling) as well as an event loop with support
//! for timers.
//!
//! [quiche]: https://github.com/cloudflare/quiche/
//! [ietf]: https://quicwg.org/
//!
//! ## Configuring connections
//!
//! The first step in establishing a QUIC connection using quiche is creating a
//! [`Config`] object:
//!
//! ```
//! let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
//! config.set_application_protos(&[b"example-proto"]);
//!
//! // Additional configuration specific to application and use case...
//! # Ok::<(), quiche::Error>(())
//! ```
//!
//! The [`Config`] object controls important aspects of the QUIC connection such
//! as QUIC version, ALPN IDs, flow control, congestion control, idle timeout
//! and other properties or features.
//!
//! QUIC is a general-purpose transport protocol and there are several
//! configuration properties where there is no reasonable default value. For
//! example, the permitted number of concurrent streams of any particular type
//! is dependent on the application running over QUIC, and other use-case
//! specific concerns.
//!
//! quiche defaults several properties to zero, applications most likely need
//! to set these to something else to satisfy their needs using the following:
//!
//! - [`set_initial_max_streams_bidi()`]
//! - [`set_initial_max_streams_uni()`]
//! - [`set_initial_max_data()`]
//! - [`set_initial_max_stream_data_bidi_local()`]
//! - [`set_initial_max_stream_data_bidi_remote()`]
//! - [`set_initial_max_stream_data_uni()`]
//!
//! [`Config`] also holds TLS configuration. This can be changed by mutators on
//! the an existing object, or by constructing a TLS context manually and
//! creating a configuration using [`with_boring_ssl_ctx_builder()`].
//!
//! A configuration object can be shared among multiple connections.
//!
//! ### Connection setup
//!
//! On the client-side the [`connect()`] utility function can be used to create
//! a new connection, while [`accept()`] is for servers:
//!
//! ```
//! # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
//! # let server_name = "quic.tech";
//! # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
//! # let peer = "127.0.0.1:1234".parse().unwrap();
//! # let local = "127.0.0.1:4321".parse().unwrap();
//! // Client connection.
//! let conn =
//!     quiche::connect(Some(&server_name), &scid, local, peer, &mut config)?;
//!
//! // Server connection.
//! # let peer = "127.0.0.1:1234".parse().unwrap();
//! # let local = "127.0.0.1:4321".parse().unwrap();
//! let conn = quiche::accept(&scid, None, local, peer, &mut config)?;
//! # Ok::<(), quiche::Error>(())
//! ```
//!
//! In both cases, the application is responsible for generating a new source
//! connection ID that will be used to identify the new connection.
//!
//! The application also need to pass the address of the remote peer of the
//! connection: in the case of a client that would be the address of the server
//! it is trying to connect to, and for a server that is the address of the
//! client that initiated the connection.
//!
//! ## Handling incoming packets
//!
//! Using the connection's [`recv()`] method the application can process
//! incoming packets that belong to that connection from the network:
//!
//! ```no_run
//! # let mut buf = [0; 512];
//! # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
//! # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
//! # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
//! # let peer = "127.0.0.1:1234".parse().unwrap();
//! # let local = "127.0.0.1:4321".parse().unwrap();
//! # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
//! let to = socket.local_addr().unwrap();
//!
//! loop {
//!     let (read, from) = socket.recv_from(&mut buf).unwrap();
//!
//!     let recv_info = quiche::RecvInfo { from, to };
//!
//!     let read = match conn.recv(&mut buf[..read], recv_info) {
//!         Ok(v) => v,
//!
//!         Err(quiche::Error::Done) => {
//!             // Done reading.
//!             break;
//!         },
//!
//!         Err(e) => {
//!             // An error occurred, handle it.
//!             break;
//!         },
//!     };
//! }
//! # Ok::<(), quiche::Error>(())
//! ```
//!
//! The application has to pass a [`RecvInfo`] structure in order to provide
//! additional information about the received packet (such as the address it
//! was received from).
//!
//! ## Generating outgoing packets
//!
//! Outgoing packet are generated using the connection's [`send()`] method
//! instead:
//!
//! ```no_run
//! # let mut out = [0; 512];
//! # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
//! # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
//! # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
//! # let peer = "127.0.0.1:1234".parse().unwrap();
//! # let local = "127.0.0.1:4321".parse().unwrap();
//! # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
//! loop {
//!     let (write, send_info) = match conn.send(&mut out) {
//!         Ok(v) => v,
//!
//!         Err(quiche::Error::Done) => {
//!             // Done writing.
//!             break;
//!         },
//!
//!         Err(e) => {
//!             // An error occurred, handle it.
//!             break;
//!         },
//!     };
//!
//!     socket.send_to(&out[..write], &send_info.to).unwrap();
//! }
//! # Ok::<(), quiche::Error>(())
//! ```
//!
//! The application will be provided with a [`SendInfo`] structure providing
//! additional information about the newly created packet (such as the address
//! the packet should be sent to).
//!
//! When packets are sent, the application is responsible for maintaining a
//! timer to react to time-based connection events. The timer expiration can be
//! obtained using the connection's [`timeout()`] method.
//!
//! ```
//! # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
//! # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
//! # let peer = "127.0.0.1:1234".parse().unwrap();
//! # let local = "127.0.0.1:4321".parse().unwrap();
//! # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
//! let timeout = conn.timeout();
//! # Ok::<(), quiche::Error>(())
//! ```
//!
//! The application is responsible for providing a timer implementation, which
//! can be specific to the operating system or networking framework used. When
//! a timer expires, the connection's [`on_timeout()`] method should be called,
//! after which additional packets might need to be sent on the network:
//!
//! ```no_run
//! # let mut out = [0; 512];
//! # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
//! # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
//! # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
//! # let peer = "127.0.0.1:1234".parse().unwrap();
//! # let local = "127.0.0.1:4321".parse().unwrap();
//! # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
//! // Timeout expired, handle it.
//! conn.on_timeout();
//!
//! // Send more packets as needed after timeout.
//! loop {
//!     let (write, send_info) = match conn.send(&mut out) {
//!         Ok(v) => v,
//!
//!         Err(quiche::Error::Done) => {
//!             // Done writing.
//!             break;
//!         },
//!
//!         Err(e) => {
//!             // An error occurred, handle it.
//!             break;
//!         },
//!     };
//!
//!     socket.send_to(&out[..write], &send_info.to).unwrap();
//! }
//! # Ok::<(), quiche::Error>(())
//! ```
//!
//! ### Pacing
//!
//! It is recommended that applications [pace] sending of outgoing packets to
//! avoid creating packet bursts that could cause short-term congestion and
//! losses in the network.
//!
//! quiche exposes pacing hints for outgoing packets through the [`at`] field
//! of the [`SendInfo`] structure that is returned by the [`send()`] method.
//! This field represents the time when a specific packet should be sent into
//! the network.
//!
//! Applications can use these hints by artificially delaying the sending of
//! packets through platform-specific mechanisms (such as the [`SO_TXTIME`]
//! socket option on Linux), or custom methods (for example by using user-space
//! timers).
//!
//! [pace]: https://datatracker.ietf.org/doc/html/rfc9002#section-7.7
//! [`SO_TXTIME`]: https://man7.org/linux/man-pages/man8/tc-etf.8.html
//!
//! ## Sending and receiving stream data
//!
//! After some back and forth, the connection will complete its handshake and
//! will be ready for sending or receiving application data.
//!
//! Data can be sent on a stream by using the [`stream_send()`] method:
//!
//! ```no_run
//! # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
//! # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
//! # let peer = "127.0.0.1:1234".parse().unwrap();
//! # let local = "127.0.0.1:4321".parse().unwrap();
//! # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
//! if conn.is_established() {
//!     // Handshake completed, send some data on stream 0.
//!     conn.stream_send(0, b"hello", true)?;
//! }
//! # Ok::<(), quiche::Error>(())
//! ```
//!
//! The application can check whether there are any readable streams by using
//! the connection's [`readable()`] method, which returns an iterator over all
//! the streams that have outstanding data to read.
//!
//! The [`stream_recv()`] method can then be used to retrieve the application
//! data from the readable stream:
//!
//! ```no_run
//! # let mut buf = [0; 512];
//! # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
//! # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
//! # let peer = "127.0.0.1:1234".parse().unwrap();
//! # let local = "127.0.0.1:4321".parse().unwrap();
//! # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
//! if conn.is_established() {
//!     // Iterate over readable streams.
//!     for stream_id in conn.readable() {
//!         // Stream is readable, read until there's no more data.
//!         while let Ok((read, fin)) = conn.stream_recv(stream_id, &mut buf) {
//!             println!("Got {} bytes on stream {}", read, stream_id);
//!         }
//!     }
//! }
//! # Ok::<(), quiche::Error>(())
//! ```
//!
//! ## HTTP/3
//!
//! The quiche [HTTP/3 module] provides a high level API for sending and
//! receiving HTTP requests and responses on top of the QUIC transport protocol.
//!
//! [`Config`]: https://docs.quic.tech/quiche/struct.Config.html
//! [`set_initial_max_streams_bidi()`]: https://docs.rs/quiche/latest/quiche/struct.Config.html#method.set_initial_max_streams_bidi
//! [`set_initial_max_streams_uni()`]: https://docs.rs/quiche/latest/quiche/struct.Config.html#method.set_initial_max_streams_uni
//! [`set_initial_max_data()`]: https://docs.rs/quiche/latest/quiche/struct.Config.html#method.set_initial_max_data
//! [`set_initial_max_stream_data_bidi_local()`]: https://docs.rs/quiche/latest/quiche/struct.Config.html#method.set_initial_max_stream_data_bidi_local
//! [`set_initial_max_stream_data_bidi_remote()`]: https://docs.rs/quiche/latest/quiche/struct.Config.html#method.set_initial_max_stream_data_bidi_remote
//! [`set_initial_max_stream_data_uni()`]: https://docs.rs/quiche/latest/quiche/struct.Config.html#method.set_initial_max_stream_data_uni
//! [`with_boring_ssl_ctx_builder()`]: https://docs.quic.tech/quiche/struct.Config.html#method.with_boring_ssl_ctx_builder
//! [`connect()`]: fn.connect.html
//! [`accept()`]: fn.accept.html
//! [`recv()`]: struct.Connection.html#method.recv
//! [`RecvInfo`]: struct.RecvInfo.html
//! [`send()`]: struct.Connection.html#method.send
//! [`SendInfo`]: struct.SendInfo.html
//! [`at`]: struct.SendInfo.html#structfield.at
//! [`timeout()`]: struct.Connection.html#method.timeout
//! [`on_timeout()`]: struct.Connection.html#method.on_timeout
//! [`stream_send()`]: struct.Connection.html#method.stream_send
//! [`readable()`]: struct.Connection.html#method.readable
//! [`stream_recv()`]: struct.Connection.html#method.stream_recv
//! [HTTP/3 module]: h3/index.html
//!
//! ## Congestion Control
//!
//! The quiche library provides a high-level API for configuring which
//! congestion control algorithm to use throughout the QUIC connection.
//!
//! When a QUIC connection is created, the application can optionally choose
//! which CC algorithm to use. See [`CongestionControlAlgorithm`] for currently
//! available congestion control algorithms.
//!
//! For example:
//!
//! ```
//! let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION).unwrap();
//! config.set_cc_algorithm(quiche::CongestionControlAlgorithm::Reno);
//! ```
//!
//! Alternatively, you can configure the congestion control algorithm to use
//! by its name.
//!
//! ```
//! let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION).unwrap();
//! config.set_cc_algorithm_name("reno").unwrap();
//! ```
//!
//! Note that the CC algorithm should be configured before calling [`connect()`]
//! or [`accept()`]. Otherwise the connection will use a default CC algorithm.
//!
//! [`CongestionControlAlgorithm`]: enum.CongestionControlAlgorithm.html
//!
//! ## Feature flags
//!
//! quiche defines a number of [feature flags] to reduce the amount of compiled
//! code and dependencies:
//!
//! * `boringssl-vendored` (default): Build the vendored BoringSSL library.
//!
//! * `boringssl-boring-crate`: Use the BoringSSL library provided by the
//!   [boring] crate. It takes precedence over `boringssl-vendored` if both
//!   features are enabled.
//!
//! * `pkg-config-meta`: Generate pkg-config metadata file for libquiche.
//!
//! * `ffi`: Build and expose the FFI API.
//!
//! * `qlog`: Enable support for the [qlog] logging format.
//!
//! [feature flags]: https://doc.rust-lang.org/cargo/reference/manifest.html#the-features-section
//! [boring]: https://crates.io/crates/boring
//! [qlog]: https://datatracker.ietf.org/doc/html/draft-ietf-quic-qlog-main-schema

#![allow(clippy::upper_case_acronyms)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
extern crate log;

#[cfg(feature = "qlog")]
use qlog::events::connectivity::ConnectivityEventType;
#[cfg(feature = "qlog")]
use qlog::events::connectivity::TransportOwner;
#[cfg(feature = "qlog")]
use qlog::events::quic::RecoveryEventType;
#[cfg(feature = "qlog")]
use qlog::events::quic::TransportEventType;
#[cfg(feature = "qlog")]
use qlog::events::DataRecipient;
#[cfg(feature = "qlog")]
use qlog::events::Event;
#[cfg(feature = "qlog")]
use qlog::events::EventData;
#[cfg(feature = "qlog")]
use qlog::events::EventImportance;
#[cfg(feature = "qlog")]
use qlog::events::EventType;
#[cfg(feature = "qlog")]
use qlog::events::RawInfo;
use stream::StreamPriorityKey;

use std::cmp;
use std::convert::TryInto;
use std::time;

use std::sync::Arc;

use std::net::SocketAddr;

use std::str::FromStr;

use std::collections::HashSet;
use std::collections::VecDeque;

use smallvec::SmallVec;

/// The current QUIC wire version.
pub const PROTOCOL_VERSION: u32 = PROTOCOL_VERSION_V1;

/// Supported QUIC versions.
const PROTOCOL_VERSION_V1: u32 = 0x0000_0001;

/// The maximum length of a connection ID.
pub const MAX_CONN_ID_LEN: usize = crate::packet::MAX_CID_LEN as usize;

/// The minimum length of Initial packets sent by a client.
pub const MIN_CLIENT_INITIAL_LEN: usize = 1200;

#[cfg(not(feature = "fuzzing"))]
const PAYLOAD_MIN_LEN: usize = 4;

#[cfg(feature = "fuzzing")]
// Due to the fact that in fuzzing mode we use a zero-length AEAD tag (which
// would normally be 16 bytes), we need to adjust the minimum payload size to
// account for that.
const PAYLOAD_MIN_LEN: usize = 20;

// PATH_CHALLENGE (9 bytes) + AEAD tag (16 bytes).
const MIN_PROBING_SIZE: usize = 25;

const MAX_AMPLIFICATION_FACTOR: usize = 3;

// The maximum number of tracked packet number ranges that need to be acked.
//
// This represents more or less how many ack blocks can fit in a typical packet.
const MAX_ACK_RANGES: usize = 68;

// The highest possible stream ID allowed.
const MAX_STREAM_ID: u64 = 1 << 60;

// The default max_datagram_size used in congestion control.
const MAX_SEND_UDP_PAYLOAD_SIZE: usize = 1200;

// The default length of DATAGRAM queues.
const DEFAULT_MAX_DGRAM_QUEUE_LEN: usize = 0;

// The default length of PATH_CHALLENGE receive queue.
const DEFAULT_MAX_PATH_CHALLENGE_RX_QUEUE_LEN: usize = 3;

// The DATAGRAM standard recommends either none or 65536 as maximum DATAGRAM
// frames size. We enforce the recommendation for forward compatibility.
const MAX_DGRAM_FRAME_SIZE: u64 = 65536;

// The length of the payload length field.
const PAYLOAD_LENGTH_LEN: usize = 2;

// The number of undecryptable that can be buffered.
const MAX_UNDECRYPTABLE_PACKETS: usize = 10;

const RESERVED_VERSION_MASK: u32 = 0xfafafafa;

// The default size of the receiver connection flow control window.
const DEFAULT_CONNECTION_WINDOW: u64 = 48 * 1024;

// The maximum size of the receiver connection flow control window.
const MAX_CONNECTION_WINDOW: u64 = 24 * 1024 * 1024;

// How much larger the connection flow control window need to be larger than
// the stream flow control window.
const CONNECTION_WINDOW_FACTOR: f64 = 1.5;

// How many probing packet timeouts do we tolerate before considering the path
// validation as failed.
const MAX_PROBING_TIMEOUTS: usize = 3;

// The default initial congestion window size in terms of packet count.
const DEFAULT_INITIAL_CONGESTION_WINDOW_PACKETS: usize = 10;

// The maximum data offset that can be stored in a crypto stream.
const MAX_CRYPTO_STREAM_OFFSET: u64 = 1 << 16;

/// A specialized [`Result`] type for quiche operations.
///
/// This type is used throughout quiche's public API for any operation that
/// can produce an error.
///
/// [`Result`]: https://doc.rust-lang.org/std/result/enum.Result.html
pub type Result<T> = std::result::Result<T, Error>;

/// A QUIC error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// There is no more work to do.
    Done,

    /// The provided buffer is too short.
    BufferTooShort,

    /// The provided packet cannot be parsed because its version is unknown.
    UnknownVersion,

    /// The provided packet cannot be parsed because it contains an invalid
    /// frame.
    InvalidFrame,

    /// The provided packet cannot be parsed.
    InvalidPacket,

    /// The operation cannot be completed because the connection is in an
    /// invalid state.
    InvalidState,

    /// The operation cannot be completed because the stream is in an
    /// invalid state.
    ///
    /// The stream ID is provided as associated data.
    InvalidStreamState(u64),

    /// The peer's transport params cannot be parsed.
    InvalidTransportParam,

    /// A cryptographic operation failed.
    CryptoFail,

    /// The TLS handshake failed.
    TlsFail,

    /// The peer violated the local flow control limits.
    FlowControl,

    /// The peer violated the local stream limits.
    StreamLimit,

    /// The specified stream was stopped by the peer.
    ///
    /// The error code sent as part of the `STOP_SENDING` frame is provided as
    /// associated data.
    StreamStopped(u64),

    /// The specified stream was reset by the peer.
    ///
    /// The error code sent as part of the `RESET_STREAM` frame is provided as
    /// associated data.
    StreamReset(u64),

    /// The received data exceeds the stream's final size.
    FinalSize,

    /// Error in congestion control.
    CongestionControl,

    /// Too many identifiers were provided.
    IdLimit,

    /// Not enough available identifiers.
    OutOfIdentifiers,

    /// Error in key update.
    KeyUpdate,

    /// The peer sent more data in CRYPTO frames than we can buffer.
    CryptoBufferExceeded,
}

/// QUIC error codes sent on the wire.
///
/// As defined in [RFC9000](https://www.rfc-editor.org/rfc/rfc9000.html#name-error-codes).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum WireErrorCode {
    /// An endpoint uses this with CONNECTION_CLOSE to signal that the
    /// connection is being closed abruptly in the absence of any error.
    NoError              = 0x0,
    /// The endpoint encountered an internal error and cannot continue with the
    /// connection.
    InternalError        = 0x1,
    /// The server refused to accept a new connection.
    ConnectionRefused    = 0x2,
    /// An endpoint received more data than it permitted in its advertised data
    /// limits; see Section 4.
    FlowControlError     = 0x3,
    /// An endpoint received a frame for a stream identifier that exceeded its
    /// advertised stream limit for the corresponding stream type.
    StreamLimitError     = 0x4,
    /// An endpoint received a frame for a stream that was not in a state that
    /// permitted that frame.
    StreamStateError     = 0x5,
    /// (1) An endpoint received a STREAM frame containing data that exceeded
    /// the previously established final size, (2) an endpoint received a
    /// STREAM frame or a RESET_STREAM frame containing a final size that
    /// was lower than the size of stream data that was already received, or
    /// (3) an endpoint received a STREAM frame or a RESET_STREAM frame
    /// containing a different final size to the one already established.
    FinalSizeError       = 0x6,
    /// An endpoint received a frame that was badly formatted -- for instance, a
    /// frame of an unknown type or an ACK frame that has more
    /// acknowledgment ranges than the remainder of the packet could carry.
    FrameEncodingError   = 0x7,
    /// An endpoint received transport parameters that were badly formatted,
    /// included an invalid value, omitted a mandatory transport parameter,
    /// included a forbidden transport parameter, or were otherwise in
    /// error.
    TransportParameterError = 0x8,
    /// An endpoint received transport parameters that were badly formatted,
    /// included an invalid value, omitted a mandatory transport parameter,
    /// included a forbidden transport parameter, or were otherwise in
    /// error.
    ConnectionIdLimitError = 0x9,
    /// An endpoint detected an error with protocol compliance that was not
    /// covered by more specific error codes.
    ProtocolViolation    = 0xa,
    /// A server received a client Initial that contained an invalid Token
    /// field.
    InvalidToken         = 0xb,
    /// The application or application protocol caused the connection to be
    /// closed.
    ApplicationError     = 0xc,
    /// An endpoint has received more data in CRYPTO frames than it can buffer.
    CryptoBufferExceeded = 0xd,
    /// An endpoint detected errors in performing key updates.
    KeyUpdateError       = 0xe,
    /// An endpoint has reached the confidentiality or integrity limit for the
    /// AEAD algorithm used by the given connection.
    AeadLimitReached     = 0xf,
    /// An endpoint has determined that the network path is incapable of
    /// supporting QUIC. An endpoint is unlikely to receive a
    /// CONNECTION_CLOSE frame carrying this code except when the path does
    /// not support a large enough MTU.
    NoViablePath         = 0x10,
}

impl Error {
    fn to_wire(self) -> u64 {
        match self {
            Error::Done => WireErrorCode::NoError as u64,
            Error::InvalidFrame => WireErrorCode::FrameEncodingError as u64,
            Error::InvalidStreamState(..) =>
                WireErrorCode::StreamStateError as u64,
            Error::InvalidTransportParam =>
                WireErrorCode::TransportParameterError as u64,
            Error::FlowControl => WireErrorCode::FlowControlError as u64,
            Error::StreamLimit => WireErrorCode::StreamLimitError as u64,
            Error::IdLimit => WireErrorCode::ConnectionIdLimitError as u64,
            Error::FinalSize => WireErrorCode::FinalSizeError as u64,
            Error::CryptoBufferExceeded =>
                WireErrorCode::CryptoBufferExceeded as u64,
            Error::KeyUpdate => WireErrorCode::KeyUpdateError as u64,
            _ => WireErrorCode::ProtocolViolation as u64,
        }
    }

    #[cfg(feature = "ffi")]
    fn to_c(self) -> libc::ssize_t {
        match self {
            Error::Done => -1,
            Error::BufferTooShort => -2,
            Error::UnknownVersion => -3,
            Error::InvalidFrame => -4,
            Error::InvalidPacket => -5,
            Error::InvalidState => -6,
            Error::InvalidStreamState(_) => -7,
            Error::InvalidTransportParam => -8,
            Error::CryptoFail => -9,
            Error::TlsFail => -10,
            Error::FlowControl => -11,
            Error::StreamLimit => -12,
            Error::FinalSize => -13,
            Error::CongestionControl => -14,
            Error::StreamStopped { .. } => -15,
            Error::StreamReset { .. } => -16,
            Error::IdLimit => -17,
            Error::OutOfIdentifiers => -18,
            Error::KeyUpdate => -19,
            Error::CryptoBufferExceeded => -20,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl std::convert::From<octets::BufferTooShortError> for Error {
    fn from(_err: octets::BufferTooShortError) -> Self {
        Error::BufferTooShort
    }
}

/// Ancillary information about incoming packets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecvInfo {
    /// The remote address the packet was received from.
    pub from: SocketAddr,

    /// The local address the packet was received on.
    pub to: SocketAddr,
}

/// Ancillary information about outgoing packets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendInfo {
    /// The local address the packet should be sent from.
    pub from: SocketAddr,

    /// The remote address the packet should be sent to.
    pub to: SocketAddr,

    /// The time to send the packet out.
    ///
    /// See [Pacing] for more details.
    ///
    /// [Pacing]: index.html#pacing
    pub at: time::Instant,
}

/// Represents information carried by `CONNECTION_CLOSE` frames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionError {
    /// Whether the error came from the application or the transport layer.
    pub is_app: bool,

    /// The error code carried by the `CONNECTION_CLOSE` frame.
    pub error_code: u64,

    /// The reason carried by the `CONNECTION_CLOSE` frame.
    pub reason: Vec<u8>,
}

/// The side of the stream to be shut down.
///
/// This should be used when calling [`stream_shutdown()`].
///
/// [`stream_shutdown()`]: struct.Connection.html#method.stream_shutdown
#[repr(C)]
#[derive(PartialEq, Eq)]
pub enum Shutdown {
    /// Stop receiving stream data.
    Read  = 0,

    /// Stop sending stream data.
    Write = 1,
}

/// Qlog logging level.
#[repr(C)]
#[cfg(feature = "qlog")]
#[cfg_attr(docsrs, doc(cfg(feature = "qlog")))]
pub enum QlogLevel {
    /// Logs any events of Core importance.
    Core  = 0,

    /// Logs any events of Core and Base importance.
    Base  = 1,

    /// Logs any events of Core, Base and Extra importance
    Extra = 2,
}

/// Stores configuration shared between multiple connections.
pub struct Config {
    local_transport_params: TransportParams,

    version: u32,

    tls_ctx: tls::Context,

    application_protos: Vec<Vec<u8>>,

    grease: bool,

    cc_algorithm: CongestionControlAlgorithm,
    initial_congestion_window_packets: usize,

    pmtud: bool,

    hystart: bool,

    pacing: bool,
    max_pacing_rate: Option<u64>,

    dgram_recv_max_queue_len: usize,
    dgram_send_max_queue_len: usize,

    path_challenge_recv_max_queue_len: usize,

    max_send_udp_payload_size: usize,

    max_connection_window: u64,
    max_stream_window: u64,

    max_amplification_factor: usize,

    disable_dcid_reuse: bool,
}

// See https://quicwg.org/base-drafts/rfc9000.html#section-15
fn is_reserved_version(version: u32) -> bool {
    version & RESERVED_VERSION_MASK == version
}

impl Config {
    /// Creates a config object with the given version.
    ///
    /// ## Examples:
    ///
    /// ```
    /// let config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn new(version: u32) -> Result<Config> {
        Self::with_tls_ctx(version, tls::Context::new()?)
    }

    /// Creates a config object with the given version and
    /// [`SslContextBuilder`].
    ///
    /// This is useful for applications that wish to manually configure
    /// [`SslContextBuilder`].
    ///
    /// [`SslContextBuilder`]: https://docs.rs/boring/latest/boring/ssl/struct.SslContextBuilder.html
    #[cfg(feature = "boringssl-boring-crate")]
    #[cfg_attr(docsrs, doc(cfg(feature = "boringssl-boring-crate")))]
    pub fn with_boring_ssl_ctx_builder(
        version: u32, tls_ctx_builder: boring::ssl::SslContextBuilder,
    ) -> Result<Config> {
        Self::with_tls_ctx(version, tls::Context::from_boring(tls_ctx_builder))
    }

    fn with_tls_ctx(version: u32, tls_ctx: tls::Context) -> Result<Config> {
        if !is_reserved_version(version) && !version_is_supported(version) {
            return Err(Error::UnknownVersion);
        }

        Ok(Config {
            local_transport_params: TransportParams::default(),
            version,
            tls_ctx,
            application_protos: Vec::new(),
            grease: true,
            cc_algorithm: CongestionControlAlgorithm::CUBIC,
            initial_congestion_window_packets:
                DEFAULT_INITIAL_CONGESTION_WINDOW_PACKETS,
            pmtud: false,
            hystart: true,
            pacing: true,
            max_pacing_rate: None,

            dgram_recv_max_queue_len: DEFAULT_MAX_DGRAM_QUEUE_LEN,
            dgram_send_max_queue_len: DEFAULT_MAX_DGRAM_QUEUE_LEN,

            path_challenge_recv_max_queue_len:
                DEFAULT_MAX_PATH_CHALLENGE_RX_QUEUE_LEN,

            max_send_udp_payload_size: MAX_SEND_UDP_PAYLOAD_SIZE,

            max_connection_window: MAX_CONNECTION_WINDOW,
            max_stream_window: stream::MAX_STREAM_WINDOW,

            max_amplification_factor: MAX_AMPLIFICATION_FACTOR,

            disable_dcid_reuse: false,
        })
    }

    /// Configures the given certificate chain.
    ///
    /// The content of `file` is parsed as a PEM-encoded leaf certificate,
    /// followed by optional intermediate certificates.
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut config = quiche::Config::new(0xbabababa)?;
    /// config.load_cert_chain_from_pem_file("/path/to/cert.pem")?;
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn load_cert_chain_from_pem_file(&mut self, file: &str) -> Result<()> {
        self.tls_ctx.use_certificate_chain_file(file)
    }

    /// Configures the given private key.
    ///
    /// The content of `file` is parsed as a PEM-encoded private key.
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut config = quiche::Config::new(0xbabababa)?;
    /// config.load_priv_key_from_pem_file("/path/to/key.pem")?;
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn load_priv_key_from_pem_file(&mut self, file: &str) -> Result<()> {
        self.tls_ctx.use_privkey_file(file)
    }

    /// Specifies a file where trusted CA certificates are stored for the
    /// purposes of certificate verification.
    ///
    /// The content of `file` is parsed as a PEM-encoded certificate chain.
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut config = quiche::Config::new(0xbabababa)?;
    /// config.load_verify_locations_from_file("/path/to/cert.pem")?;
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn load_verify_locations_from_file(&mut self, file: &str) -> Result<()> {
        self.tls_ctx.load_verify_locations_from_file(file)
    }

    /// Specifies a directory where trusted CA certificates are stored for the
    /// purposes of certificate verification.
    ///
    /// The content of `dir` a set of PEM-encoded certificate chains.
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut config = quiche::Config::new(0xbabababa)?;
    /// config.load_verify_locations_from_directory("/path/to/certs")?;
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn load_verify_locations_from_directory(
        &mut self, dir: &str,
    ) -> Result<()> {
        self.tls_ctx.load_verify_locations_from_directory(dir)
    }

    /// Configures whether to verify the peer's certificate.
    ///
    /// The default value is `true` for client connections, and `false` for
    /// server ones.
    ///
    /// Note that on the server-side, enabling verification of the peer will
    /// trigger a certificate request and make authentication errors fatal, but
    /// will still allow anonymous clients (i.e. clients that don't present a
    /// certificate at all). Servers can check whether a client presented a
    /// certificate by calling [`peer_cert()`] if they need to.
    ///
    /// [`peer_cert()`]: struct.Connection.html#method.peer_cert
    pub fn verify_peer(&mut self, verify: bool) {
        self.tls_ctx.set_verify(verify);
    }

    /// Configures whether to do path MTU discovery.
    ///
    /// The default value is `false`.
    pub fn discover_pmtu(&mut self, discover: bool) {
        self.pmtud = discover;
    }

    /// Configures whether to send GREASE values.
    ///
    /// The default value is `true`.
    pub fn grease(&mut self, grease: bool) {
        self.grease = grease;
    }

    /// Enables logging of secrets.
    ///
    /// When logging is enabled, the [`set_keylog()`] method must be called on
    /// the connection for its cryptographic secrets to be logged in the
    /// [keylog] format to the specified writer.
    ///
    /// [`set_keylog()`]: struct.Connection.html#method.set_keylog
    /// [keylog]: https://developer.mozilla.org/en-US/docs/Mozilla/Projects/NSS/Key_Log_Format
    pub fn log_keys(&mut self) {
        self.tls_ctx.enable_keylog();
    }

    /// Configures the session ticket key material.
    ///
    /// On the server this key will be used to encrypt and decrypt session
    /// tickets, used to perform session resumption without server-side state.
    ///
    /// By default a key is generated internally, and rotated regularly, so
    /// applications don't need to call this unless they need to use a
    /// specific key (e.g. in order to support resumption across multiple
    /// servers), in which case the application is also responsible for
    /// rotating the key to provide forward secrecy.
    pub fn set_ticket_key(&mut self, key: &[u8]) -> Result<()> {
        self.tls_ctx.set_ticket_key(key)
    }

    /// Enables sending or receiving early data.
    pub fn enable_early_data(&mut self) {
        self.tls_ctx.set_early_data_enabled(true);
    }

    /// Configures the list of supported application protocols.
    ///
    /// On the client this configures the list of protocols to send to the
    /// server as part of the ALPN extension.
    ///
    /// On the server this configures the list of supported protocols to match
    /// against the client-supplied list.
    ///
    /// Applications must set a value, but no default is provided.
    ///
    /// ## Examples:
    ///
    /// ```
    /// # let mut config = quiche::Config::new(0xbabababa)?;
    /// config.set_application_protos(&[b"http/1.1", b"http/0.9"]);
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn set_application_protos(
        &mut self, protos_list: &[&[u8]],
    ) -> Result<()> {
        self.application_protos =
            protos_list.iter().map(|s| s.to_vec()).collect();

        self.tls_ctx.set_alpn(protos_list)
    }

    /// Configures the list of supported application protocols using wire
    /// format.
    ///
    /// The list of protocols `protos` must be a series of non-empty, 8-bit
    /// length-prefixed strings.
    ///
    /// See [`set_application_protos`](Self::set_application_protos) for more
    /// background about application protocols.
    ///
    /// ## Examples:
    ///
    /// ```
    /// # let mut config = quiche::Config::new(0xbabababa)?;
    /// config.set_application_protos_wire_format(b"\x08http/1.1\x08http/0.9")?;
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn set_application_protos_wire_format(
        &mut self, protos: &[u8],
    ) -> Result<()> {
        let mut b = octets::Octets::with_slice(protos);

        let mut protos_list = Vec::new();

        while let Ok(proto) = b.get_bytes_with_u8_length() {
            protos_list.push(proto.buf());
        }

        self.set_application_protos(&protos_list)
    }

    /// Sets the anti-amplification limit factor.
    ///
    /// The default value is `3`.
    pub fn set_max_amplification_factor(&mut self, v: usize) {
        self.max_amplification_factor = v;
    }

    /// Sets the `max_idle_timeout` transport parameter, in milliseconds.
    ///
    /// The default value is infinite, that is, no timeout is used.
    pub fn set_max_idle_timeout(&mut self, v: u64) {
        self.local_transport_params.max_idle_timeout = v;
    }

    /// Sets the `max_udp_payload_size transport` parameter.
    ///
    /// The default value is `65527`.
    pub fn set_max_recv_udp_payload_size(&mut self, v: usize) {
        self.local_transport_params.max_udp_payload_size = v as u64;
    }

    /// Sets the maximum outgoing UDP payload size.
    ///
    /// The default and minimum value is `1200`.
    pub fn set_max_send_udp_payload_size(&mut self, v: usize) {
        self.max_send_udp_payload_size = cmp::max(v, MAX_SEND_UDP_PAYLOAD_SIZE);
    }

    /// Sets the `initial_max_data` transport parameter.
    ///
    /// When set to a non-zero value quiche will only allow at most `v` bytes of
    /// incoming stream data to be buffered for the whole connection (that is,
    /// data that is not yet read by the application) and will allow more data
    /// to be received as the buffer is consumed by the application.
    ///
    /// When set to zero, either explicitly or via the default, quiche will not
    /// give any flow control to the peer, preventing it from sending any stream
    /// data.
    ///
    /// The default value is `0`.
    pub fn set_initial_max_data(&mut self, v: u64) {
        self.local_transport_params.initial_max_data = v;
    }

    /// Sets the `initial_max_stream_data_bidi_local` transport parameter.
    ///
    /// When set to a non-zero value quiche will only allow at most `v` bytes
    /// of incoming stream data to be buffered for each locally-initiated
    /// bidirectional stream (that is, data that is not yet read by the
    /// application) and will allow more data to be received as the buffer is
    /// consumed by the application.
    ///
    /// When set to zero, either explicitly or via the default, quiche will not
    /// give any flow control to the peer, preventing it from sending any stream
    /// data.
    ///
    /// The default value is `0`.
    pub fn set_initial_max_stream_data_bidi_local(&mut self, v: u64) {
        self.local_transport_params
            .initial_max_stream_data_bidi_local = v;
    }

    /// Sets the `initial_max_stream_data_bidi_remote` transport parameter.
    ///
    /// When set to a non-zero value quiche will only allow at most `v` bytes
    /// of incoming stream data to be buffered for each remotely-initiated
    /// bidirectional stream (that is, data that is not yet read by the
    /// application) and will allow more data to be received as the buffer is
    /// consumed by the application.
    ///
    /// When set to zero, either explicitly or via the default, quiche will not
    /// give any flow control to the peer, preventing it from sending any stream
    /// data.
    ///
    /// The default value is `0`.
    pub fn set_initial_max_stream_data_bidi_remote(&mut self, v: u64) {
        self.local_transport_params
            .initial_max_stream_data_bidi_remote = v;
    }

    /// Sets the `initial_max_stream_data_uni` transport parameter.
    ///
    /// When set to a non-zero value quiche will only allow at most `v` bytes
    /// of incoming stream data to be buffered for each unidirectional stream
    /// (that is, data that is not yet read by the application) and will allow
    /// more data to be received as the buffer is consumed by the application.
    ///
    /// When set to zero, either explicitly or via the default, quiche will not
    /// give any flow control to the peer, preventing it from sending any stream
    /// data.
    ///
    /// The default value is `0`.
    pub fn set_initial_max_stream_data_uni(&mut self, v: u64) {
        self.local_transport_params.initial_max_stream_data_uni = v;
    }

    /// Sets the `initial_max_streams_bidi` transport parameter.
    ///
    /// When set to a non-zero value quiche will only allow `v` number of
    /// concurrent remotely-initiated bidirectional streams to be open at any
    /// given time and will increase the limit automatically as streams are
    /// completed.
    ///
    /// When set to zero, either explicitly or via the default, quiche will not
    /// not allow the peer to open any bidirectional streams.
    ///
    /// A bidirectional stream is considered completed when all incoming data
    /// has been read by the application (up to the `fin` offset) or the
    /// stream's read direction has been shutdown, and all outgoing data has
    /// been acked by the peer (up to the `fin` offset) or the stream's write
    /// direction has been shutdown.
    ///
    /// The default value is `0`.
    pub fn set_initial_max_streams_bidi(&mut self, v: u64) {
        self.local_transport_params.initial_max_streams_bidi = v;
    }

    /// Sets the `initial_max_streams_uni` transport parameter.
    ///
    /// When set to a non-zero value quiche will only allow `v` number of
    /// concurrent remotely-initiated unidirectional streams to be open at any
    /// given time and will increase the limit automatically as streams are
    /// completed.
    ///
    /// When set to zero, either explicitly or via the default, quiche will not
    /// not allow the peer to open any unidirectional streams.
    ///
    /// A unidirectional stream is considered completed when all incoming data
    /// has been read by the application (up to the `fin` offset) or the
    /// stream's read direction has been shutdown.
    ///
    /// The default value is `0`.
    pub fn set_initial_max_streams_uni(&mut self, v: u64) {
        self.local_transport_params.initial_max_streams_uni = v;
    }

    /// Sets the `ack_delay_exponent` transport parameter.
    ///
    /// The default value is `3`.
    pub fn set_ack_delay_exponent(&mut self, v: u64) {
        self.local_transport_params.ack_delay_exponent = v;
    }

    /// Sets the `max_ack_delay` transport parameter.
    ///
    /// The default value is `25`.
    pub fn set_max_ack_delay(&mut self, v: u64) {
        self.local_transport_params.max_ack_delay = v;
    }

    /// Sets the `active_connection_id_limit` transport parameter.
    ///
    /// The default value is `2`. Lower values will be ignored.
    pub fn set_active_connection_id_limit(&mut self, v: u64) {
        if v >= 2 {
            self.local_transport_params.active_conn_id_limit = v;
        }
    }

    /// Sets the `disable_active_migration` transport parameter.
    ///
    /// The default value is `false`.
    pub fn set_disable_active_migration(&mut self, v: bool) {
        self.local_transport_params.disable_active_migration = v;
    }

    /// Sets the congestion control algorithm used by string.
    ///
    /// The default value is `cubic`. On error `Error::CongestionControl`
    /// will be returned.
    ///
    /// ## Examples:
    ///
    /// ```
    /// # let mut config = quiche::Config::new(0xbabababa)?;
    /// config.set_cc_algorithm_name("reno");
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn set_cc_algorithm_name(&mut self, name: &str) -> Result<()> {
        self.cc_algorithm = CongestionControlAlgorithm::from_str(name)?;

        Ok(())
    }

    /// Sets initial congestion window size in terms of packet count.
    ///
    /// The default value is 10.
    pub fn set_initial_congestion_window_packets(&mut self, packets: usize) {
        self.initial_congestion_window_packets = packets;
    }

    /// Sets the congestion control algorithm used.
    ///
    /// The default value is `CongestionControlAlgorithm::CUBIC`.
    pub fn set_cc_algorithm(&mut self, algo: CongestionControlAlgorithm) {
        self.cc_algorithm = algo;
    }

    /// Configures whether to enable HyStart++.
    ///
    /// The default value is `true`.
    pub fn enable_hystart(&mut self, v: bool) {
        self.hystart = v;
    }

    /// Configures whether to enable pacing.
    ///
    /// The default value is `true`.
    pub fn enable_pacing(&mut self, v: bool) {
        self.pacing = v;
    }

    /// Sets the max value for pacing rate.
    ///
    /// By default pacing rate is not limited.
    pub fn set_max_pacing_rate(&mut self, v: u64) {
        self.max_pacing_rate = Some(v);
    }

    /// Configures whether to enable receiving DATAGRAM frames.
    ///
    /// When enabled, the `max_datagram_frame_size` transport parameter is set
    /// to 65536 as recommended by draft-ietf-quic-datagram-01.
    ///
    /// The default is `false`.
    pub fn enable_dgram(
        &mut self, enabled: bool, recv_queue_len: usize, send_queue_len: usize,
    ) {
        self.local_transport_params.max_datagram_frame_size = if enabled {
            Some(MAX_DGRAM_FRAME_SIZE)
        } else {
            None
        };
        self.dgram_recv_max_queue_len = recv_queue_len;
        self.dgram_send_max_queue_len = send_queue_len;
    }

    /// Configures the max number of queued received PATH_CHALLENGE frames.
    ///
    /// When an endpoint receives a PATH_CHALLENGE frame and the queue is full,
    /// the frame is discarded.
    ///
    /// The default is 3.
    pub fn set_path_challenge_recv_max_queue_len(&mut self, queue_len: usize) {
        self.path_challenge_recv_max_queue_len = queue_len;
    }

    /// Sets the maximum size of the connection window.
    ///
    /// The default value is MAX_CONNECTION_WINDOW (24MBytes).
    pub fn set_max_connection_window(&mut self, v: u64) {
        self.max_connection_window = v;
    }

    /// Sets the maximum size of the stream window.
    ///
    /// The default value is MAX_STREAM_WINDOW (16MBytes).
    pub fn set_max_stream_window(&mut self, v: u64) {
        self.max_stream_window = v;
    }

    /// Sets the initial stateless reset token.
    ///
    /// This value is only advertised by servers. Setting a stateless retry
    /// token as a client has no effect on the connection.
    ///
    /// The default value is `None`.
    pub fn set_stateless_reset_token(&mut self, v: Option<u128>) {
        self.local_transport_params.stateless_reset_token = v;
    }

    /// Sets whether the QUIC connection should avoid reusing DCIDs over
    /// different paths.
    ///
    /// When set to `true`, it ensures that a destination Connection ID is never
    /// reused on different paths. Such behaviour may lead to connection stall
    /// if the peer performs a non-voluntary migration (e.g., NAT rebinding) and
    /// does not provide additional destination Connection IDs to handle such
    /// event.
    ///
    /// The default value is `false`.
    pub fn set_disable_dcid_reuse(&mut self, v: bool) {
        self.disable_dcid_reuse = v;
    }
}

/// A QUIC connection.
pub struct Connection {
    /// QUIC wire version used for the connection.
    version: u32,

    /// Connection Identifiers.
    ids: cid::ConnectionIdentifiers,

    /// Unique opaque ID for the connection that can be used for logging.
    trace_id: String,

    /// Packet number spaces.
    pkt_num_spaces: [packet::PktNumSpace; packet::Epoch::count()],

    /// Peer's transport parameters.
    peer_transport_params: TransportParams,

    /// Local transport parameters.
    local_transport_params: TransportParams,

    /// TLS handshake state.
    handshake: tls::Handshake,

    /// Serialized TLS session buffer.
    ///
    /// This field is populated when a new session ticket is processed on the
    /// client. On the server this is empty.
    session: Option<Vec<u8>>,

    /// The configuration for recovery.
    recovery_config: recovery::RecoveryConfig,

    /// The path manager.
    paths: path::PathMap,

    /// PATH_CHALLENGE receive queue max length.
    path_challenge_recv_max_queue_len: usize,

    /// Total number of received PATH_CHALLENGE frames.
    path_challenge_rx_count: u64,

    /// List of supported application protocols.
    application_protos: Vec<Vec<u8>>,

    /// Total number of received packets.
    recv_count: usize,

    /// Total number of sent packets.
    sent_count: usize,

    /// Total number of lost packets.
    lost_count: usize,

    /// Total number of packets sent with data retransmitted.
    retrans_count: usize,

    /// Total number of bytes received from the peer.
    rx_data: u64,

    /// Receiver flow controller.
    flow_control: flowcontrol::FlowControl,

    /// Whether we send MAX_DATA frame.
    almost_full: bool,

    /// Number of stream data bytes that can be buffered.
    tx_cap: usize,

    // Number of bytes buffered in the send buffer.
    tx_buffered: usize,

    /// Total number of bytes sent to the peer.
    tx_data: u64,

    /// Peer's flow control limit for the connection.
    max_tx_data: u64,

    /// Last tx_data before running a full send() loop.
    last_tx_data: u64,

    /// Total number of bytes retransmitted over the connection.
    /// This counts only STREAM and CRYPTO data.
    stream_retrans_bytes: u64,

    /// Total number of bytes sent over the connection.
    sent_bytes: u64,

    /// Total number of bytes received over the connection.
    recv_bytes: u64,

    /// Total number of bytes sent acked over the connection.
    acked_bytes: u64,

    /// Total number of bytes sent lost over the connection.
    lost_bytes: u64,

    /// Streams map, indexed by stream ID.
    streams: stream::StreamMap,

    /// Peer's original destination connection ID. Used by the client to
    /// validate the server's transport parameter.
    odcid: Option<ConnectionId<'static>>,

    /// Peer's retry source connection ID. Used by the client during stateless
    /// retry to validate the server's transport parameter.
    rscid: Option<ConnectionId<'static>>,

    /// Received address verification token.
    token: Option<Vec<u8>>,

    /// Error code and reason to be sent to the peer in a CONNECTION_CLOSE
    /// frame.
    local_error: Option<ConnectionError>,

    /// Error code and reason received from the peer in a CONNECTION_CLOSE
    /// frame.
    peer_error: Option<ConnectionError>,

    /// The connection-level limit at which send blocking occurred.
    blocked_limit: Option<u64>,

    /// Idle timeout expiration time.
    idle_timer: Option<time::Instant>,

    /// Draining timeout expiration time.
    draining_timer: Option<time::Instant>,

    /// List of raw packets that were received before they could be decrypted.
    undecryptable_pkts: VecDeque<(Vec<u8>, RecvInfo)>,

    /// The negotiated ALPN protocol.
    alpn: Vec<u8>,

    /// Whether this is a server-side connection.
    is_server: bool,

    /// Whether the initial secrets have been derived.
    derived_initial_secrets: bool,

    /// Whether a version negotiation packet has already been received. Only
    /// relevant for client connections.
    did_version_negotiation: bool,

    /// Whether stateless retry has been performed.
    did_retry: bool,

    /// Whether the peer already updated its connection ID.
    got_peer_conn_id: bool,

    /// Whether the peer verified our initial address.
    peer_verified_initial_address: bool,

    /// Whether the peer's transport parameters were parsed.
    parsed_peer_transport_params: bool,

    /// Whether the connection handshake has been completed.
    handshake_completed: bool,

    /// Whether the HANDSHAKE_DONE frame has been sent.
    handshake_done_sent: bool,

    /// Whether the HANDSHAKE_DONE frame has been acked.
    handshake_done_acked: bool,

    /// Whether the connection handshake has been confirmed.
    handshake_confirmed: bool,

    /// Key phase bit used for outgoing protected packets.
    key_phase: bool,

    /// Whether an ack-eliciting packet has been sent since last receiving a
    /// packet.
    ack_eliciting_sent: bool,

    /// Whether the connection is closed.
    closed: bool,

    /// Whether the connection was timed out.
    timed_out: bool,

    /// Whether to send GREASE.
    grease: bool,

    /// TLS keylog writer.
    keylog: Option<Box<dyn std::io::Write + Send + Sync>>,

    #[cfg(feature = "qlog")]
    qlog: QlogInfo,

    /// DATAGRAM queues.
    dgram_recv_queue: dgram::DatagramQueue,
    dgram_send_queue: dgram::DatagramQueue,

    /// Whether to emit DATAGRAM frames in the next packet.
    emit_dgram: bool,

    /// Whether the connection should prevent from reusing destination
    /// Connection IDs when the peer migrates.
    disable_dcid_reuse: bool,

    /// The number of streams reset by local.
    reset_stream_local_count: u64,

    /// The number of streams stopped by local.
    stopped_stream_local_count: u64,

    /// The number of streams reset by remote.
    reset_stream_remote_count: u64,

    /// The number of streams stopped by remote.
    stopped_stream_remote_count: u64,

    /// The anti-amplification limit factor.
    max_amplification_factor: usize,

    /// Optimist acknowledgment extension.
    /// Flow control scaling factor.
    oack_scaling_factor: Option<u64>,

    /// Optimist acknowledgment extension.
    /// The peer might start with a random packet number. Log it.
    oack_first_app_pn: Option<u64>,

    /// Optimistic acknowledgment extension.
    /// The maximum true received packet number.
    oack_max_recv_pn: u64,
}

/// Creates a new server-side connection.
///
/// The `scid` parameter represents the server's source connection ID, while
/// the optional `odcid` parameter represents the original destination ID the
/// client sent before a stateless retry (this is only required when using
/// the [`retry()`] function).
///
/// [`retry()`]: fn.retry.html
///
/// ## Examples:
///
/// ```no_run
/// # let mut config = quiche::Config::new(0xbabababa)?;
/// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
/// # let local = "127.0.0.1:0".parse().unwrap();
/// # let peer = "127.0.0.1:1234".parse().unwrap();
/// let conn = quiche::accept(&scid, None, local, peer, &mut config)?;
/// # Ok::<(), quiche::Error>(())
/// ```
#[inline]
pub fn accept(
    scid: &ConnectionId, odcid: Option<&ConnectionId>, local: SocketAddr,
    peer: SocketAddr, config: &mut Config,
) -> Result<Connection> {
    let conn = Connection::new(scid, odcid, local, peer, config, true)?;

    Ok(conn)
}

/// Creates a new client-side connection.
///
/// The `scid` parameter is used as the connection's source connection ID,
/// while the optional `server_name` parameter is used to verify the peer's
/// certificate.
///
/// ## Examples:
///
/// ```no_run
/// # let mut config = quiche::Config::new(0xbabababa)?;
/// # let server_name = "quic.tech";
/// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
/// # let local = "127.0.0.1:4321".parse().unwrap();
/// # let peer = "127.0.0.1:1234".parse().unwrap();
/// let conn =
///     quiche::connect(Some(&server_name), &scid, local, peer, &mut config)?;
/// # Ok::<(), quiche::Error>(())
/// ```
#[inline]
pub fn connect(
    server_name: Option<&str>, scid: &ConnectionId, local: SocketAddr,
    peer: SocketAddr, config: &mut Config,
) -> Result<Connection> {
    let mut conn = Connection::new(scid, None, local, peer, config, false)?;

    if let Some(server_name) = server_name {
        conn.handshake.set_host_name(server_name)?;
    }

    Ok(conn)
}

/// Writes a version negotiation packet.
///
/// The `scid` and `dcid` parameters are the source connection ID and the
/// destination connection ID extracted from the received client's Initial
/// packet that advertises an unsupported version.
///
/// ## Examples:
///
/// ```no_run
/// # let mut buf = [0; 512];
/// # let mut out = [0; 512];
/// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
/// let (len, src) = socket.recv_from(&mut buf).unwrap();
///
/// let hdr =
///     quiche::Header::from_slice(&mut buf[..len], quiche::MAX_CONN_ID_LEN)?;
///
/// if hdr.version != quiche::PROTOCOL_VERSION {
///     let len = quiche::negotiate_version(&hdr.scid, &hdr.dcid, &mut out)?;
///     socket.send_to(&out[..len], &src).unwrap();
/// }
/// # Ok::<(), quiche::Error>(())
/// ```
#[inline]
pub fn negotiate_version(
    scid: &ConnectionId, dcid: &ConnectionId, out: &mut [u8],
) -> Result<usize> {
    packet::negotiate_version(scid, dcid, out)
}

/// Writes a stateless retry packet.
///
/// The `scid` and `dcid` parameters are the source connection ID and the
/// destination connection ID extracted from the received client's Initial
/// packet, while `new_scid` is the server's new source connection ID and
/// `token` is the address validation token the client needs to echo back.
///
/// The application is responsible for generating the address validation
/// token to be sent to the client, and verifying tokens sent back by the
/// client. The generated token should include the `dcid` parameter, such
/// that it can be later extracted from the token and passed to the
/// [`accept()`] function as its `odcid` parameter.
///
/// [`accept()`]: fn.accept.html
///
/// ## Examples:
///
/// ```no_run
/// # let mut config = quiche::Config::new(0xbabababa)?;
/// # let mut buf = [0; 512];
/// # let mut out = [0; 512];
/// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
/// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
/// # let local = socket.local_addr().unwrap();
/// # fn mint_token(hdr: &quiche::Header, src: &std::net::SocketAddr) -> Vec<u8> {
/// #     vec![]
/// # }
/// # fn validate_token<'a>(src: &std::net::SocketAddr, token: &'a [u8]) -> Option<quiche::ConnectionId<'a>> {
/// #     None
/// # }
/// let (len, peer) = socket.recv_from(&mut buf).unwrap();
///
/// let hdr = quiche::Header::from_slice(&mut buf[..len], quiche::MAX_CONN_ID_LEN)?;
///
/// let token = hdr.token.as_ref().unwrap();
///
/// // No token sent by client, create a new one.
/// if token.is_empty() {
///     let new_token = mint_token(&hdr, &peer);
///
///     let len = quiche::retry(
///         &hdr.scid, &hdr.dcid, &scid, &new_token, hdr.version, &mut out,
///     )?;
///
///     socket.send_to(&out[..len], &peer).unwrap();
///     return Ok(());
/// }
///
/// // Client sent token, validate it.
/// let odcid = validate_token(&peer, token);
///
/// if odcid.is_none() {
///     // Invalid address validation token.
///     return Ok(());
/// }
///
/// let conn = quiche::accept(&scid, odcid.as_ref(), local, peer, &mut config)?;
/// # Ok::<(), quiche::Error>(())
/// ```
#[inline]
pub fn retry(
    scid: &ConnectionId, dcid: &ConnectionId, new_scid: &ConnectionId,
    token: &[u8], version: u32, out: &mut [u8],
) -> Result<usize> {
    packet::retry(scid, dcid, new_scid, token, version, out)
}

/// Returns true if the given protocol version is supported.
#[inline]
pub fn version_is_supported(version: u32) -> bool {
    matches!(version, PROTOCOL_VERSION_V1)
}

/// Pushes a frame to the output packet if there is enough space.
///
/// Returns `true` on success, `false` otherwise. In case of failure it means
/// there is no room to add the frame in the packet. You may retry to add the
/// frame later.
macro_rules! push_frame_to_pkt {
    ($out:expr, $frames:expr, $frame:expr, $left:expr) => {{
        if $frame.wire_len() <= $left {
            $left -= $frame.wire_len();

            $frame.to_bytes(&mut $out)?;

            $frames.push($frame);

            true
        } else {
            false
        }
    }};
}

/// Executes the provided body if the qlog feature is enabled, quiche has been
/// configured with a log writer, the event's importance is within the
/// configured level.
macro_rules! qlog_with_type {
    ($ty:expr, $qlog:expr, $qlog_streamer_ref:ident, $body:block) => {{
        #[cfg(feature = "qlog")]
        {
            if EventImportance::from($ty).is_contained_in(&$qlog.level) {
                if let Some($qlog_streamer_ref) = &mut $qlog.streamer {
                    $body
                }
            }
        }
    }};
}

#[cfg(feature = "qlog")]
const QLOG_PARAMS_SET: EventType =
    EventType::TransportEventType(TransportEventType::ParametersSet);

#[cfg(feature = "qlog")]
const QLOG_PACKET_RX: EventType =
    EventType::TransportEventType(TransportEventType::PacketReceived);

#[cfg(feature = "qlog")]
const QLOG_PACKET_TX: EventType =
    EventType::TransportEventType(TransportEventType::PacketSent);

#[cfg(feature = "qlog")]
const QLOG_DATA_MV: EventType =
    EventType::TransportEventType(TransportEventType::DataMoved);

#[cfg(feature = "qlog")]
const QLOG_METRICS: EventType =
    EventType::RecoveryEventType(RecoveryEventType::MetricsUpdated);

#[cfg(feature = "qlog")]
const QLOG_CONNECTION_CLOSED: EventType =
    EventType::ConnectivityEventType(ConnectivityEventType::ConnectionClosed);

#[cfg(feature = "qlog")]
struct QlogInfo {
    streamer: Option<qlog::streamer::QlogStreamer>,
    logged_peer_params: bool,
    level: EventImportance,
}

#[cfg(feature = "qlog")]
impl Default for QlogInfo {
    fn default() -> Self {
        QlogInfo {
            streamer: None,
            logged_peer_params: false,
            level: EventImportance::Base,
        }
    }
}

impl Connection {
    fn new(
        scid: &ConnectionId, odcid: Option<&ConnectionId>, local: SocketAddr,
        peer: SocketAddr, config: &mut Config, is_server: bool,
    ) -> Result<Connection> {
        let tls = config.tls_ctx.new_handshake()?;
        Connection::with_tls(scid, odcid, local, peer, config, tls, is_server)
    }

    fn with_tls(
        scid: &ConnectionId, odcid: Option<&ConnectionId>, local: SocketAddr,
        peer: SocketAddr, config: &Config, tls: tls::Handshake, is_server: bool,
    ) -> Result<Connection> {
        let max_rx_data = config.local_transport_params.initial_max_data;

        let scid_as_hex: Vec<String> =
            scid.iter().map(|b| format!("{b:02x}")).collect();

        let reset_token = if is_server {
            config.local_transport_params.stateless_reset_token
        } else {
            None
        };

        let recovery_config = recovery::RecoveryConfig::from_config(config);

        let mut path = path::Path::new(
            local,
            peer,
            &recovery_config,
            config.path_challenge_recv_max_queue_len,
            MIN_CLIENT_INITIAL_LEN,
            true,
        );

        // If we did stateless retry assume the peer's address is verified.
        path.verified_peer_address = odcid.is_some();
        // Assume clients validate the server's address implicitly.
        path.peer_verified_local_address = is_server;

        // Do not allocate more than the number of active CIDs.
        let paths = path::PathMap::new(
            path,
            config.local_transport_params.active_conn_id_limit as usize,
            is_server,
            config.pmtud,
            config.max_send_udp_payload_size,
        );

        let active_path_id = paths.get_active_path_id()?;

        let ids = cid::ConnectionIdentifiers::new(
            config.local_transport_params.active_conn_id_limit as usize,
            scid,
            active_path_id,
            reset_token,
        );

        let mut conn = Connection {
            version: config.version,

            ids,

            trace_id: scid_as_hex.join(""),

            pkt_num_spaces: [
                packet::PktNumSpace::new(),
                packet::PktNumSpace::new(),
                packet::PktNumSpace::new(),
            ],

            peer_transport_params: TransportParams::default(),

            local_transport_params: config.local_transport_params.clone(),

            handshake: tls,

            session: None,

            recovery_config,

            paths,
            path_challenge_recv_max_queue_len: config
                .path_challenge_recv_max_queue_len,
            path_challenge_rx_count: 0,

            application_protos: config.application_protos.clone(),

            recv_count: 0,
            sent_count: 0,
            lost_count: 0,
            retrans_count: 0,
            sent_bytes: 0,
            recv_bytes: 0,
            acked_bytes: 0,
            lost_bytes: 0,

            rx_data: 0,
            flow_control: flowcontrol::FlowControl::new(
                max_rx_data,
                cmp::min(max_rx_data / 2 * 3, DEFAULT_CONNECTION_WINDOW),
                config.max_connection_window,
            ),
            almost_full: false,

            tx_cap: 0,

            tx_buffered: 0,

            tx_data: 0,
            max_tx_data: 0,
            last_tx_data: 0,

            stream_retrans_bytes: 0,

            streams: stream::StreamMap::new(
                config.local_transport_params.initial_max_streams_bidi,
                config.local_transport_params.initial_max_streams_uni,
                config.max_stream_window,
            ),

            odcid: None,

            rscid: None,

            token: None,

            local_error: None,

            peer_error: None,

            blocked_limit: None,

            idle_timer: None,

            draining_timer: None,

            undecryptable_pkts: VecDeque::new(),

            alpn: Vec::new(),

            is_server,

            derived_initial_secrets: false,

            did_version_negotiation: false,

            did_retry: false,

            got_peer_conn_id: false,

            // Assume clients validate the server's address implicitly.
            peer_verified_initial_address: is_server,

            parsed_peer_transport_params: false,

            handshake_completed: false,

            handshake_done_sent: false,
            handshake_done_acked: false,

            handshake_confirmed: false,

            key_phase: false,

            ack_eliciting_sent: false,

            closed: false,

            timed_out: false,

            grease: config.grease,

            keylog: None,

            #[cfg(feature = "qlog")]
            qlog: Default::default(),

            dgram_recv_queue: dgram::DatagramQueue::new(
                config.dgram_recv_max_queue_len,
            ),

            dgram_send_queue: dgram::DatagramQueue::new(
                config.dgram_send_max_queue_len,
            ),

            emit_dgram: true,

            disable_dcid_reuse: config.disable_dcid_reuse,

            reset_stream_local_count: 0,
            stopped_stream_local_count: 0,
            reset_stream_remote_count: 0,
            stopped_stream_remote_count: 0,

            max_amplification_factor: config.max_amplification_factor,

            oack_scaling_factor: None,
            oack_first_app_pn: None,
            oack_max_recv_pn: 0,
        };

        if let Some(odcid) = odcid {
            conn.local_transport_params
                .original_destination_connection_id = Some(odcid.to_vec().into());

            conn.local_transport_params.retry_source_connection_id =
                Some(conn.ids.get_scid(0)?.cid.to_vec().into());

            conn.did_retry = true;
        }

        conn.local_transport_params.initial_source_connection_id =
            Some(conn.ids.get_scid(0)?.cid.to_vec().into());

        conn.handshake.init(is_server)?;

        conn.handshake
            .use_legacy_codepoint(config.version != PROTOCOL_VERSION_V1);

        conn.encode_transport_params()?;

        // Derive initial secrets for the client. We can do this here because
        // we already generated the random destination connection ID.
        if !is_server {
            let mut dcid = [0; 16];
            rand::rand_bytes(&mut dcid[..]);

            let (aead_open, aead_seal) = crypto::derive_initial_key_material(
                &dcid,
                conn.version,
                conn.is_server,
            )?;

            let reset_token = conn.peer_transport_params.stateless_reset_token;
            conn.set_initial_dcid(
                dcid.to_vec().into(),
                reset_token,
                active_path_id,
            )?;

            conn.pkt_num_spaces[packet::Epoch::Initial].crypto_open =
                Some(aead_open);
            conn.pkt_num_spaces[packet::Epoch::Initial].crypto_seal =
                Some(aead_seal);

            conn.derived_initial_secrets = true;
        }

        Ok(conn)
    }

    /// Sets keylog output to the designated [`Writer`].
    ///
    /// This needs to be called as soon as the connection is created, to avoid
    /// missing some early logs.
    ///
    /// [`Writer`]: https://doc.rust-lang.org/std/io/trait.Write.html
    #[inline]
    pub fn set_keylog(&mut self, writer: Box<dyn std::io::Write + Send + Sync>) {
        self.keylog = Some(writer);
    }

    /// Sets qlog output to the designated [`Writer`].
    ///
    /// Only events included in `QlogLevel::Base` are written. The serialization
    /// format is JSON-SEQ.
    ///
    /// This needs to be called as soon as the connection is created, to avoid
    /// missing some early logs.
    ///
    /// [`Writer`]: https://doc.rust-lang.org/std/io/trait.Write.html
    #[cfg(feature = "qlog")]
    #[cfg_attr(docsrs, doc(cfg(feature = "qlog")))]
    pub fn set_qlog(
        &mut self, writer: Box<dyn std::io::Write + Send + Sync>, title: String,
        description: String,
    ) {
        self.set_qlog_with_level(writer, title, description, QlogLevel::Base)
    }

    /// Sets qlog output to the designated [`Writer`].
    ///
    /// Only qlog events included in the specified `QlogLevel` are written. The
    /// serialization format is JSON-SEQ.
    ///
    /// This needs to be called as soon as the connection is created, to avoid
    /// missing some early logs.
    ///
    /// [`Writer`]: https://doc.rust-lang.org/std/io/trait.Write.html
    #[cfg(feature = "qlog")]
    #[cfg_attr(docsrs, doc(cfg(feature = "qlog")))]
    pub fn set_qlog_with_level(
        &mut self, writer: Box<dyn std::io::Write + Send + Sync>, title: String,
        description: String, qlog_level: QlogLevel,
    ) {
        let vp = if self.is_server {
            qlog::VantagePointType::Server
        } else {
            qlog::VantagePointType::Client
        };

        let level = match qlog_level {
            QlogLevel::Core => EventImportance::Core,

            QlogLevel::Base => EventImportance::Base,

            QlogLevel::Extra => EventImportance::Extra,
        };

        self.qlog.level = level;

        let trace = qlog::TraceSeq::new(
            qlog::VantagePoint {
                name: None,
                ty: vp,
                flow: None,
            },
            Some(title.to_string()),
            Some(description.to_string()),
            Some(qlog::Configuration {
                time_offset: Some(0.0),
                original_uris: None,
            }),
            None,
        );

        let mut streamer = qlog::streamer::QlogStreamer::new(
            qlog::QLOG_VERSION.to_string(),
            Some(title),
            Some(description),
            None,
            time::Instant::now(),
            trace,
            self.qlog.level,
            writer,
        );

        streamer.start_log().ok();

        let ev_data = self
            .local_transport_params
            .to_qlog(TransportOwner::Local, self.handshake.cipher());

        // This event occurs very early, so just mark the relative time as 0.0.
        streamer.add_event(Event::with_time(0.0, ev_data)).ok();

        self.qlog.streamer = Some(streamer);
    }

    /// Returns a mutable reference to the QlogStreamer, if it exists.
    #[cfg(feature = "qlog")]
    #[cfg_attr(docsrs, doc(cfg(feature = "qlog")))]
    pub fn qlog_streamer(&mut self) -> Option<&mut qlog::streamer::QlogStreamer> {
        self.qlog.streamer.as_mut()
    }

    /// Configures the given session for resumption.
    ///
    /// On the client, this can be used to offer the given serialized session,
    /// as returned by [`session()`], for resumption.
    ///
    /// This must only be called immediately after creating a connection, that
    /// is, before any packet is sent or received.
    ///
    /// [`session()`]: struct.Connection.html#method.session
    #[inline]
    pub fn set_session(&mut self, session: &[u8]) -> Result<()> {
        let mut b = octets::Octets::with_slice(session);

        let session_len = b.get_u64()? as usize;
        let session_bytes = b.get_bytes(session_len)?;

        self.handshake.set_session(session_bytes.as_ref())?;

        let raw_params_len = b.get_u64()? as usize;
        let raw_params_bytes = b.get_bytes(raw_params_len)?;

        let peer_params =
            TransportParams::decode(raw_params_bytes.as_ref(), self.is_server)?;

        self.process_peer_transport_params(peer_params)?;

        Ok(())
    }

    /// Processes QUIC packets received from the peer.
    ///
    /// On success the number of bytes processed from the input buffer is
    /// returned. On error the connection will be closed by calling [`close()`]
    /// with the appropriate error code.
    ///
    /// Coalesced packets will be processed as necessary.
    ///
    /// Note that the contents of the input buffer `buf` might be modified by
    /// this function due to, for example, in-place decryption.
    ///
    /// [`close()`]: struct.Connection.html#method.close
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut buf = [0; 512];
    /// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    /// # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    /// # let peer = "127.0.0.1:1234".parse().unwrap();
    /// # let local = socket.local_addr().unwrap();
    /// # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
    /// loop {
    ///     let (read, from) = socket.recv_from(&mut buf).unwrap();
    ///
    ///     let recv_info = quiche::RecvInfo {
    ///         from,
    ///         to: local,
    ///     };
    ///
    ///     let read = match conn.recv(&mut buf[..read], recv_info) {
    ///         Ok(v) => v,
    ///
    ///         Err(e) => {
    ///             // An error occurred, handle it.
    ///             break;
    ///         },
    ///     };
    /// }
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn recv(&mut self, buf: &mut [u8], info: RecvInfo) -> Result<usize> {
        let len = buf.len();

        if len == 0 {
            return Err(Error::BufferTooShort);
        }

        let recv_pid = self.paths.path_id_from_addrs(&(info.to, info.from));

        if let Some(recv_pid) = recv_pid {
            let recv_path = self.paths.get_mut(recv_pid)?;

            // Keep track of how many bytes we received from the client, so we
            // can limit bytes sent back before address validation, to a
            // multiple of this. The limit needs to be increased early on, so
            // that if there is an error there is enough credit to send a
            // CONNECTION_CLOSE.
            //
            // It doesn't matter if the packets received were valid or not, we
            // only need to track the total amount of bytes received.
            //
            // Note that we also need to limit the number of bytes we sent on a
            // path if we are not the host that initiated its usage.
            if self.is_server && !recv_path.verified_peer_address {
                recv_path.max_send_bytes += len * self.max_amplification_factor;
            }
        } else if !self.is_server {
            // If a client receives packets from an unknown server address,
            // the client MUST discard these packets.
            trace!(
                "{} client received packet from unknown address {:?}, dropping",
                self.trace_id,
                info,
            );

            return Ok(len);
        }

        let mut done = 0;
        let mut left = len;

        // Process coalesced packets.
        while left > 0 {
            let read = match self.recv_single(
                &mut buf[len - left..len],
                &info,
                recv_pid,
            ) {
                Ok(v) => v,

                Err(Error::Done) => {
                    // If the packet can't be processed or decrypted, check if
                    // it's a stateless reset.
                    if self.is_stateless_reset(&buf[len - left..len]) {
                        trace!("{} packet is a stateless reset", self.trace_id);

                        self.mark_closed();
                    }

                    left
                },

                Err(e) => {
                    // In case of error processing the incoming packet, close
                    // the connection.
                    self.close(false, e.to_wire(), b"").ok();
                    return Err(e);
                },
            };

            done += read;
            left -= read;
        }

        // Even though the packet was previously "accepted", it
        // should be safe to forward the error, as it also comes
        // from the `recv()` method.
        self.process_undecrypted_0rtt_packets()?;

        Ok(done)
    }

    fn process_undecrypted_0rtt_packets(&mut self) -> Result<()> {
        // Process previously undecryptable 0-RTT packets if the decryption key
        // is now available.
        if self.pkt_num_spaces[packet::Epoch::Application]
            .crypto_0rtt_open
            .is_some()
        {
            while let Some((mut pkt, info)) = self.undecryptable_pkts.pop_front()
            {
                if let Err(e) = self.recv(&mut pkt, info) {
                    self.undecryptable_pkts.clear();

                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Returns true if a QUIC packet is a stateless reset.
    fn is_stateless_reset(&self, buf: &[u8]) -> bool {
        // If the packet is too small, then we just throw it away.
        let buf_len = buf.len();
        if buf_len < 21 {
            return false;
        }

        // TODO: we should iterate over all active destination connection IDs
        // and check against their reset token.
        match self.peer_transport_params.stateless_reset_token {
            Some(token) => {
                let token_len = 16;
                ring::constant_time::verify_slices_are_equal(
                    &token.to_be_bytes(),
                    &buf[buf_len - token_len..buf_len],
                )
                .is_ok()
            },

            None => false,
        }
    }

    /// Processes a single QUIC packet received from the peer.
    ///
    /// On success the number of bytes processed from the input buffer is
    /// returned. When the [`Done`] error is returned, processing of the
    /// remainder of the incoming UDP datagram should be interrupted.
    ///
    /// Note that a server might observe a new 4-tuple, preventing to
    /// know in advance to which path the incoming packet belongs to (`recv_pid`
    /// is `None`). As a client, packets from unknown 4-tuple are dropped
    /// beforehand (see `recv()`).
    ///
    /// On error, an error other than [`Done`] is returned.
    ///
    /// [`Done`]: enum.Error.html#variant.Done
    fn recv_single(
        &mut self, buf: &mut [u8], info: &RecvInfo, recv_pid: Option<usize>,
    ) -> Result<usize> {
        let now = time::Instant::now();

        if buf.is_empty() {
            return Err(Error::Done);
        }

        if self.is_closed() || self.is_draining() {
            return Err(Error::Done);
        }

        let is_closing = self.local_error.is_some();

        if is_closing {
            return Err(Error::Done);
        }

        let buf_len = buf.len();

        let mut b = octets::OctetsMut::with_slice(buf);

        let mut hdr = Header::from_bytes(&mut b, self.source_id().len())
            .map_err(|e| {
                drop_pkt_on_err(
                    e,
                    self.recv_count,
                    self.is_server,
                    &self.trace_id,
                )
            })?;

        if hdr.ty == packet::Type::VersionNegotiation {
            // Version negotiation packets can only be sent by the server.
            if self.is_server {
                return Err(Error::Done);
            }

            // Ignore duplicate version negotiation.
            if self.did_version_negotiation {
                return Err(Error::Done);
            }

            // Ignore version negotiation if any other packet has already been
            // successfully processed.
            if self.recv_count > 0 {
                return Err(Error::Done);
            }

            if hdr.dcid != self.source_id() {
                return Err(Error::Done);
            }

            if hdr.scid != self.destination_id() {
                return Err(Error::Done);
            }

            trace!("{} rx pkt {:?}", self.trace_id, hdr);

            let versions = hdr.versions.ok_or(Error::Done)?;

            // Ignore version negotiation if the version already selected is
            // listed.
            if versions.iter().any(|&v| v == self.version) {
                return Err(Error::Done);
            }

            let supported_versions =
                versions.iter().filter(|&&v| version_is_supported(v));

            let mut found_version = false;

            for &v in supported_versions {
                found_version = true;

                // The final version takes precedence over draft ones.
                if v == PROTOCOL_VERSION_V1 {
                    self.version = v;
                    break;
                }

                self.version = cmp::max(self.version, v);
            }

            if !found_version {
                // We don't support any of the versions offered.
                //
                // While a man-in-the-middle attacker might be able to
                // inject a version negotiation packet that triggers this
                // failure, the window of opportunity is very small and
                // this error is quite useful for debugging, so don't just
                // ignore the packet.
                return Err(Error::UnknownVersion);
            }

            self.did_version_negotiation = true;

            // Derive Initial secrets based on the new version.
            let (aead_open, aead_seal) = crypto::derive_initial_key_material(
                &self.destination_id(),
                self.version,
                self.is_server,
            )?;

            // Reset connection state to force sending another Initial packet.
            self.drop_epoch_state(packet::Epoch::Initial, now);
            self.got_peer_conn_id = false;
            self.handshake.clear()?;

            self.pkt_num_spaces[packet::Epoch::Initial].crypto_open =
                Some(aead_open);
            self.pkt_num_spaces[packet::Epoch::Initial].crypto_seal =
                Some(aead_seal);

            self.handshake
                .use_legacy_codepoint(self.version != PROTOCOL_VERSION_V1);

            // Encode transport parameters again, as the new version might be
            // using a different format.
            self.encode_transport_params()?;

            return Err(Error::Done);
        }

        if hdr.ty == packet::Type::Retry {
            // Retry packets can only be sent by the server.
            if self.is_server {
                return Err(Error::Done);
            }

            // Ignore duplicate retry.
            if self.did_retry {
                return Err(Error::Done);
            }

            // Check if Retry packet is valid.
            if packet::verify_retry_integrity(
                &b,
                &self.destination_id(),
                self.version,
            )
            .is_err()
            {
                return Err(Error::Done);
            }

            trace!("{} rx pkt {:?}", self.trace_id, hdr);

            self.token = hdr.token;
            self.did_retry = true;

            // Remember peer's new connection ID.
            self.odcid = Some(self.destination_id().into_owned());

            self.set_initial_dcid(
                hdr.scid.clone(),
                None,
                self.paths.get_active_path_id()?,
            )?;

            self.rscid = Some(self.destination_id().into_owned());

            // Derive Initial secrets using the new connection ID.
            let (aead_open, aead_seal) = crypto::derive_initial_key_material(
                &hdr.scid,
                self.version,
                self.is_server,
            )?;

            // Reset connection state to force sending another Initial packet.
            self.drop_epoch_state(packet::Epoch::Initial, now);
            self.got_peer_conn_id = false;
            self.handshake.clear()?;

            self.pkt_num_spaces[packet::Epoch::Initial].crypto_open =
                Some(aead_open);
            self.pkt_num_spaces[packet::Epoch::Initial].crypto_seal =
                Some(aead_seal);

            return Err(Error::Done);
        }

        if self.is_server && !self.did_version_negotiation {
            if !version_is_supported(hdr.version) {
                return Err(Error::UnknownVersion);
            }

            self.version = hdr.version;
            self.did_version_negotiation = true;

            self.handshake
                .use_legacy_codepoint(self.version != PROTOCOL_VERSION_V1);

            // Encode transport parameters again, as the new version might be
            // using a different format.
            self.encode_transport_params()?;
        }

        if hdr.ty != packet::Type::Short && hdr.version != self.version {
            // At this point version negotiation was already performed, so
            // ignore packets that don't match the connection's version.
            return Err(Error::Done);
        }

        // Long header packets have an explicit payload length, but short
        // packets don't so just use the remaining capacity in the buffer.
        let payload_len = if hdr.ty == packet::Type::Short {
            b.cap()
        } else {
            b.get_varint().map_err(|e| {
                drop_pkt_on_err(
                    e.into(),
                    self.recv_count,
                    self.is_server,
                    &self.trace_id,
                )
            })? as usize
        };

        // Make sure the buffer is same or larger than an explicit
        // payload length.
        if payload_len > b.cap() {
            return Err(drop_pkt_on_err(
                Error::InvalidPacket,
                self.recv_count,
                self.is_server,
                &self.trace_id,
            ));
        }

        // Derive initial secrets on the server.
        if !self.derived_initial_secrets {
            let (aead_open, aead_seal) = crypto::derive_initial_key_material(
                &hdr.dcid,
                self.version,
                self.is_server,
            )?;

            self.pkt_num_spaces[packet::Epoch::Initial].crypto_open =
                Some(aead_open);
            self.pkt_num_spaces[packet::Epoch::Initial].crypto_seal =
                Some(aead_seal);

            self.derived_initial_secrets = true;
        }

        // Select packet number space epoch based on the received packet's type.
        let epoch = hdr.ty.to_epoch()?;

        // Select AEAD context used to open incoming packet.
        let aead = if hdr.ty == packet::Type::ZeroRTT {
            // Only use 0-RTT key if incoming packet is 0-RTT.
            self.pkt_num_spaces[epoch].crypto_0rtt_open.as_ref()
        } else {
            // Otherwise use the packet number space's main key.
            self.pkt_num_spaces[epoch].crypto_open.as_ref()
        };

        // Finally, discard packet if no usable key is available.
        let mut aead = match aead {
            Some(v) => v,

            None => {
                if hdr.ty == packet::Type::ZeroRTT &&
                    self.undecryptable_pkts.len() < MAX_UNDECRYPTABLE_PACKETS &&
                    !self.is_established()
                {
                    // Buffer 0-RTT packets when the required read key is not
                    // available yet, and process them later.
                    //
                    // TODO: in the future we might want to buffer other types
                    // of undecryptable packets as well.
                    let pkt_len = b.off() + payload_len;
                    let pkt = (b.buf()[..pkt_len]).to_vec();

                    self.undecryptable_pkts.push_back((pkt, *info));
                    return Ok(pkt_len);
                }

                let e = drop_pkt_on_err(
                    Error::CryptoFail,
                    self.recv_count,
                    self.is_server,
                    &self.trace_id,
                );

                return Err(e);
            },
        };

        let aead_tag_len = aead.alg().tag_len();

        packet::decrypt_hdr(&mut b, &mut hdr, aead).map_err(|e| {
            drop_pkt_on_err(e, self.recv_count, self.is_server, &self.trace_id)
        })?;

        let pn = packet::decode_pkt_num(
            self.pkt_num_spaces[epoch].largest_rx_pkt_num,
            hdr.pkt_num,
            hdr.pkt_num_len,
        );

        let pn_len = hdr.pkt_num_len;

        trace!(
            "{} rx pkt {:?} len={} pn={} {}",
            self.trace_id,
            hdr,
            payload_len,
            pn,
            AddrTupleFmt(info.from, info.to)
        );

        #[cfg(feature = "qlog")]
        let mut qlog_frames = vec![];

        // Check for key update.
        let mut aead_next = None;

        if self.handshake_confirmed &&
            hdr.ty != Type::ZeroRTT &&
            hdr.key_phase != self.key_phase
        {
            // Check if this packet arrived before key update.
            if let Some(key_update) = self.pkt_num_spaces[epoch]
                .key_update
                .as_ref()
                .and_then(|key_update| {
                    (pn < key_update.pn_on_update).then_some(key_update)
                })
            {
                aead = &key_update.crypto_open;
            } else {
                trace!("{} peer-initiated key update", self.trace_id);

                aead_next = Some((
                    self.pkt_num_spaces[epoch]
                        .crypto_open
                        .as_ref()
                        .unwrap()
                        .derive_next_packet_key()?,
                    self.pkt_num_spaces[epoch]
                        .crypto_seal
                        .as_ref()
                        .unwrap()
                        .derive_next_packet_key()?,
                ));

                // `aead_next` is always `Some()` at this point, so the `unwrap()`
                // will never fail.
                aead = &aead_next.as_ref().unwrap().0;
            }
        }

        let mut payload = packet::decrypt_pkt(
            &mut b,
            pn,
            pn_len,
            payload_len,
            aead,
        )
        .map_err(|e| {
            drop_pkt_on_err(e, self.recv_count, self.is_server, &self.trace_id)
        })?;

        self.oack_max_recv_pn = cmp::max(self.oack_max_recv_pn, pn);

        if self.pkt_num_spaces[epoch].recv_pkt_num.contains(pn) {
            trace!("{} ignored duplicate packet {}", self.trace_id, pn);
            return Err(Error::Done);
        }

        // Insert the newly received packet number in the oack extension.
        self.oack_insert_range_pn(pn..pn + 1, epoch);

        // Packets with no frames are invalid.
        if payload.cap() == 0 {
            return Err(Error::InvalidPacket);
        }

        // Now that we decrypted the packet, let's see if we can map it to an
        // existing path.
        let recv_pid = if hdr.ty == packet::Type::Short && self.got_peer_conn_id {
            let pkt_dcid = ConnectionId::from_ref(&hdr.dcid);
            self.get_or_create_recv_path_id(recv_pid, &pkt_dcid, buf_len, info)?
        } else {
            // During handshake, we are on the initial path.
            self.paths.get_active_path_id()?
        };

        // The key update is verified once a packet is successfully decrypted
        // using the new keys.
        if let Some((open_next, seal_next)) = aead_next {
            if !self.pkt_num_spaces[epoch]
                .key_update
                .as_ref()
                .map_or(true, |prev| prev.update_acked)
            {
                // Peer has updated keys twice without awaiting confirmation.
                return Err(Error::KeyUpdate);
            }

            trace!("{} key update verified", self.trace_id);

            let _ = self.pkt_num_spaces[epoch].crypto_seal.replace(seal_next);

            let open_prev = self.pkt_num_spaces[epoch]
                .crypto_open
                .replace(open_next)
                .unwrap();

            let recv_path = self.paths.get_mut(recv_pid)?;

            self.pkt_num_spaces[epoch].key_update = Some(packet::KeyUpdate {
                crypto_open: open_prev,
                pn_on_update: pn,
                update_acked: false,
                timer: now + (recv_path.recovery.pto() * 3),
            });

            self.key_phase = !self.key_phase;

            qlog_with_type!(QLOG_PACKET_RX, self.qlog, q, {
                let trigger = Some(
                    qlog::events::security::KeyUpdateOrRetiredTrigger::RemoteUpdate,
                );

                let ev_data_client =
                    EventData::KeyUpdated(qlog::events::security::KeyUpdated {
                        key_type:
                            qlog::events::security::KeyType::Client1RttSecret,
                        old: None,
                        new: String::new(),
                        generation: None,
                        trigger: trigger.clone(),
                    });

                q.add_event_data_with_instant(ev_data_client, now).ok();

                let ev_data_server =
                    EventData::KeyUpdated(qlog::events::security::KeyUpdated {
                        key_type:
                            qlog::events::security::KeyType::Server1RttSecret,
                        old: None,
                        new: String::new(),
                        generation: None,
                        trigger,
                    });

                q.add_event_data_with_instant(ev_data_server, now).ok();
            });
        }

        if !self.is_server && !self.got_peer_conn_id {
            if self.odcid.is_none() {
                self.odcid = Some(self.destination_id().into_owned());
            }

            // Replace the randomly generated destination connection ID with
            // the one supplied by the server.
            self.set_initial_dcid(
                hdr.scid.clone(),
                self.peer_transport_params.stateless_reset_token,
                recv_pid,
            )?;

            self.got_peer_conn_id = true;
        }

        if self.is_server && !self.got_peer_conn_id {
            self.set_initial_dcid(hdr.scid.clone(), None, recv_pid)?;

            if !self.did_retry {
                self.local_transport_params
                    .original_destination_connection_id =
                    Some(hdr.dcid.to_vec().into());

                self.encode_transport_params()?;
            }

            self.got_peer_conn_id = true;
        }

        // To avoid sending an ACK in response to an ACK-only packet, we need
        // to keep track of whether this packet contains any frame other than
        // ACK and PADDING.
        let mut ack_elicited = false;

        // Process packet payload. If a frame cannot be processed, store the
        // error and stop further packet processing.
        let mut frame_processing_err = None;

        // To know if the peer migrated the connection, we need to keep track
        // whether this is a non-probing packet.
        let mut probing = true;

        // Process packet payload.
        while payload.cap() > 0 {
            let frame = frame::Frame::from_bytes(&mut payload, hdr.ty)?;

            qlog_with_type!(QLOG_PACKET_RX, self.qlog, _q, {
                qlog_frames.push(frame.to_qlog());
            });

            if frame.ack_eliciting() {
                ack_elicited = true;
            }

            if !frame.probing() {
                probing = false;
            }

            if let Err(e) = self.process_frame(frame, &hdr, recv_pid, epoch, now)
            {
                frame_processing_err = Some(e);
                break;
            }
        }

        qlog_with_type!(QLOG_PACKET_RX, self.qlog, q, {
            let packet_size = b.len();

            let qlog_pkt_hdr = qlog::events::quic::PacketHeader::with_type(
                hdr.ty.to_qlog(),
                Some(pn),
                Some(hdr.version),
                Some(&hdr.scid),
                Some(&hdr.dcid),
            );

            let qlog_raw_info = RawInfo {
                length: Some(packet_size as u64),
                payload_length: Some(payload_len as u64),
                data: None,
            };

            let ev_data =
                EventData::PacketReceived(qlog::events::quic::PacketReceived {
                    header: qlog_pkt_hdr,
                    frames: Some(qlog_frames),
                    is_coalesced: None,
                    retry_token: None,
                    stateless_reset_token: None,
                    supported_versions: None,
                    raw: Some(qlog_raw_info),
                    datagram_id: None,
                    trigger: None,
                });

            q.add_event_data_with_instant(ev_data, now).ok();
        });

        qlog_with_type!(QLOG_PACKET_RX, self.qlog, q, {
            let recv_path = self.paths.get_mut(recv_pid)?;
            if let Some(ev_data) = recv_path.recovery.maybe_qlog() {
                q.add_event_data_with_instant(ev_data, now).ok();
            }
        });

        if let Some(e) = frame_processing_err {
            // Any frame error is terminal, so now just return.
            return Err(e);
        }

        // Only log the remote transport parameters once the connection is
        // established (i.e. after frames have been fully parsed) and only
        // once per connection.
        if self.is_established() {
            qlog_with_type!(QLOG_PARAMS_SET, self.qlog, q, {
                if !self.qlog.logged_peer_params {
                    let ev_data = self
                        .peer_transport_params
                        .to_qlog(TransportOwner::Remote, self.handshake.cipher());

                    q.add_event_data_with_instant(ev_data, now).ok();

                    self.qlog.logged_peer_params = true;
                }
            });
        }

        // Following flag used to upgrade datagram size, if probe is successful.
        let mut pmtud_probe = false;

        // Process acked frames. Note that several packets from several paths
        // might have been acked by the received packet.
        for (_, p) in self.paths.iter_mut() {
            for acked in p.recovery.get_acked_frames(epoch) {
                match acked {
                    frame::Frame::Ping {
                        mtu_probe: Some(mtu_probe),
                    } => {
                        let pmtud_next = p.pmtud.get_current();
                        p.pmtud.set_current(cmp::max(pmtud_next, mtu_probe));

                        // Stop sending path MTU probes after successful probe.
                        p.pmtud.should_probe(false);
                        pmtud_probe = true;

                        trace!(
                            "{} pmtud acked; pmtu size {:?}",
                            self.trace_id,
                            p.pmtud.get_current()
                        );
                    },

                    frame::Frame::ACK { ranges, .. } => {
                        // Stop acknowledging packets less than or equal to the
                        // largest acknowledged in the sent ACK frame that, in
                        // turn, got acked.
                        if let Some(largest_acked) = ranges.last() {
                            self.pkt_num_spaces[epoch]
                                .recv_pkt_need_ack
                                .remove_until(largest_acked);
                        }
                    },

                    frame::Frame::CryptoHeader { offset, length } => {
                        self.pkt_num_spaces[epoch]
                            .crypto_stream
                            .send
                            .ack_and_drop(offset, length);
                    },

                    frame::Frame::StreamHeader {
                        stream_id,
                        offset,
                        length,
                        ..
                    } => {
                        let stream = match self.streams.get_mut(stream_id) {
                            Some(v) => v,

                            None => continue,
                        };

                        stream.send.ack_and_drop(offset, length);

                        self.tx_buffered =
                            self.tx_buffered.saturating_sub(length);

                        qlog_with_type!(QLOG_DATA_MV, self.qlog, q, {
                            let ev_data = EventData::DataMoved(
                                qlog::events::quic::DataMoved {
                                    stream_id: Some(stream_id),
                                    offset: Some(offset),
                                    length: Some(length as u64),
                                    from: Some(DataRecipient::Transport),
                                    to: Some(DataRecipient::Dropped),
                                    raw: None,
                                },
                            );

                            q.add_event_data_with_instant(ev_data, now).ok();
                        });

                        // Only collect the stream if it is complete and not
                        // readable. If it is readable, it will get collected when
                        // stream_recv() is used.
                        if stream.is_complete() && !stream.is_readable() {
                            let local = stream.local;
                            self.streams.collect(stream_id, local);
                        }
                    },

                    frame::Frame::HandshakeDone => {
                        // Explicitly set this to true, so that if the frame was
                        // already scheduled for retransmission, it is aborted.
                        self.handshake_done_sent = true;

                        self.handshake_done_acked = true;
                    },

                    frame::Frame::ResetStream { stream_id, .. } => {
                        let stream = match self.streams.get_mut(stream_id) {
                            Some(v) => v,

                            None => continue,
                        };

                        // Only collect the stream if it is complete and not
                        // readable. If it is readable, it will get collected when
                        // stream_recv() is used.
                        if stream.is_complete() && !stream.is_readable() {
                            let local = stream.local;
                            self.streams.collect(stream_id, local);
                        }
                    },

                    _ => (),
                }
            }

            // Update max datagram send size with newly acked probe size.
            if pmtud_probe {
                trace!(
                    "{} updating pmtu {:?}",
                    p.pmtud.get_current(),
                    self.trace_id
                );

                qlog_with_type!(
                    EventType::ConnectivityEventType(
                        ConnectivityEventType::MtuUpdated
                    ),
                    self.qlog,
                    q,
                    {
                        let pmtu_data = EventData::MtuUpdated(
                            qlog::events::connectivity::MtuUpdated {
                                old: Some(p.recovery.max_datagram_size() as u16),
                                new: p.pmtud.get_current() as u16,
                                done: Some(pmtud_probe),
                            },
                        );

                        q.add_event_data_with_instant(pmtu_data, now).ok();
                    }
                );

                p.recovery
                    .pmtud_update_max_datagram_size(p.pmtud.get_current());
            }
        }

        // Now that we processed all the frames, if there is a path that has no
        // Destination CID, try to allocate one.
        let no_dcid = self
            .paths
            .iter_mut()
            .filter(|(_, p)| p.active_dcid_seq.is_none());

        for (pid, p) in no_dcid {
            if self.ids.zero_length_dcid() {
                p.active_dcid_seq = Some(0);
                continue;
            }

            let dcid_seq = match self.ids.lowest_available_dcid_seq() {
                Some(seq) => seq,
                None => break,
            };

            self.ids.link_dcid_to_path_id(dcid_seq, pid)?;

            p.active_dcid_seq = Some(dcid_seq);
        }

        // We only record the time of arrival of the largest packet number
        // that still needs to be acked, to be used for ACK delay calculation.
        if self.pkt_num_spaces[epoch].recv_pkt_need_ack.last() < Some(pn) {
            self.pkt_num_spaces[epoch].largest_rx_pkt_time = now;
        }

        if self.pkt_num_spaces[epoch].largest_rx_pkt_num + 1 != pn {
            println!("SEEING A GAP IN THE PACKET NUMBER SEQUENCE!! {} vs {}", self.pkt_num_spaces[epoch].largest_rx_pkt_num, pn);
            if self.oack_first_app_pn.is_none() && epoch == packet::Epoch::Application {
                self.oack_first_app_pn = Some(pn);
            }
        }

        self.pkt_num_spaces[epoch].recv_pkt_num.insert(pn);

        self.pkt_num_spaces[epoch].recv_pkt_need_ack.push_item(pn);

        self.pkt_num_spaces[epoch].ack_elicited =
            cmp::max(self.pkt_num_spaces[epoch].ack_elicited, ack_elicited);

        self.pkt_num_spaces[epoch].largest_rx_pkt_num =
            cmp::max(self.pkt_num_spaces[epoch].largest_rx_pkt_num, pn);

        if !probing {
            self.pkt_num_spaces[epoch].largest_rx_non_probing_pkt_num = cmp::max(
                self.pkt_num_spaces[epoch].largest_rx_non_probing_pkt_num,
                pn,
            );

            // Did the peer migrated to another path?
            let active_path_id = self.paths.get_active_path_id()?;

            if self.is_server &&
                recv_pid != active_path_id &&
                self.pkt_num_spaces[epoch].largest_rx_non_probing_pkt_num == pn
            {
                self.on_peer_migrated(recv_pid, self.disable_dcid_reuse, now)?;
            }
        }

        if let Some(idle_timeout) = self.idle_timeout() {
            self.idle_timer = Some(now + idle_timeout);
        }

        // Update send capacity.
        self.update_tx_cap();

        self.recv_count += 1;
        self.paths.get_mut(recv_pid)?.recv_count += 1;

        let read = b.off() + aead_tag_len;

        self.recv_bytes += read as u64;
        self.paths.get_mut(recv_pid)?.recv_bytes += read as u64;

        // An Handshake packet has been received from the client and has been
        // successfully processed, so we can drop the initial state and consider
        // the client's address to be verified.
        if self.is_server && hdr.ty == packet::Type::Handshake {
            self.drop_epoch_state(packet::Epoch::Initial, now);

            self.paths.get_mut(recv_pid)?.verified_peer_address = true;
        }

        self.ack_eliciting_sent = false;

        Ok(read)
    }

    /// Writes a single QUIC packet to be sent to the peer.
    ///
    /// On success the number of bytes written to the output buffer is
    /// returned, or [`Done`] if there was nothing to write.
    ///
    /// The application should call `send()` multiple times until [`Done`] is
    /// returned, indicating that there are no more packets to send. It is
    /// recommended that `send()` be called in the following cases:
    ///
    ///  * When the application receives QUIC packets from the peer (that is,
    ///    any time [`recv()`] is also called).
    ///
    ///  * When the connection timer expires (that is, any time [`on_timeout()`]
    ///    is also called).
    ///
    ///  * When the application sends data to the peer (for example, any time
    ///    [`stream_send()`] or [`stream_shutdown()`] are called).
    ///
    ///  * When the application receives data from the peer (for example any
    ///    time [`stream_recv()`] is called).
    ///
    /// Once [`is_draining()`] returns `true`, it is no longer necessary to call
    /// `send()` and all calls will return [`Done`].
    ///
    /// [`Done`]: enum.Error.html#variant.Done
    /// [`recv()`]: struct.Connection.html#method.recv
    /// [`on_timeout()`]: struct.Connection.html#method.on_timeout
    /// [`stream_send()`]: struct.Connection.html#method.stream_send
    /// [`stream_shutdown()`]: struct.Connection.html#method.stream_shutdown
    /// [`stream_recv()`]: struct.Connection.html#method.stream_recv
    /// [`is_draining()`]: struct.Connection.html#method.is_draining
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut out = [0; 512];
    /// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    /// # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    /// # let peer = "127.0.0.1:1234".parse().unwrap();
    /// # let local = socket.local_addr().unwrap();
    /// # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
    /// loop {
    ///     let (write, send_info) = match conn.send(&mut out) {
    ///         Ok(v) => v,
    ///
    ///         Err(quiche::Error::Done) => {
    ///             // Done writing.
    ///             break;
    ///         },
    ///
    ///         Err(e) => {
    ///             // An error occurred, handle it.
    ///             break;
    ///         },
    ///     };
    ///
    ///     socket.send_to(&out[..write], &send_info.to).unwrap();
    /// }
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn send(&mut self, out: &mut [u8]) -> Result<(usize, SendInfo)> {
        self.send_on_path(out, None, None)
    }

    /// Writes a single QUIC packet to be sent to the peer from the specified
    /// local address `from` to the destination address `to`.
    ///
    /// The behavior of this method differs depending on the value of the `from`
    /// and `to` parameters:
    ///
    ///  * If both are `Some`, then the method only consider the 4-tuple
    ///    (`from`, `to`). Application can monitor the 4-tuple availability,
    ///    either by monitoring [`path_event_next()`] events or by relying on
    ///    the [`paths_iter()`] method. If the provided 4-tuple does not exist
    ///    on the connection (anymore), it returns an [`InvalidState`].
    ///
    ///  * If `from` is `Some` and `to` is `None`, then the method only
    ///    considers sending packets on paths having `from` as local address.
    ///
    ///  * If `to` is `Some` and `from` is `None`, then the method only
    ///    considers sending packets on paths having `to` as peer address.
    ///
    ///  * If both are `None`, all available paths are considered.
    ///
    /// On success the number of bytes written to the output buffer is
    /// returned, or [`Done`] if there was nothing to write.
    ///
    /// The application should call `send_on_path()` multiple times until
    /// [`Done`] is returned, indicating that there are no more packets to
    /// send. It is recommended that `send_on_path()` be called in the
    /// following cases:
    ///
    ///  * When the application receives QUIC packets from the peer (that is,
    ///    any time [`recv()`] is also called).
    ///
    ///  * When the connection timer expires (that is, any time [`on_timeout()`]
    ///    is also called).
    ///
    ///  * When the application sends data to the peer (for examples, any time
    ///    [`stream_send()`] or [`stream_shutdown()`] are called).
    ///
    ///  * When the application receives data from the peer (for example any
    ///    time [`stream_recv()`] is called).
    ///
    /// Once [`is_draining()`] returns `true`, it is no longer necessary to call
    /// `send_on_path()` and all calls will return [`Done`].
    ///
    /// [`Done`]: enum.Error.html#variant.Done
    /// [`InvalidState`]: enum.Error.html#InvalidState
    /// [`recv()`]: struct.Connection.html#method.recv
    /// [`on_timeout()`]: struct.Connection.html#method.on_timeout
    /// [`stream_send()`]: struct.Connection.html#method.stream_send
    /// [`stream_shutdown()`]: struct.Connection.html#method.stream_shutdown
    /// [`stream_recv()`]: struct.Connection.html#method.stream_recv
    /// [`path_event_next()`]: struct.Connection.html#method.path_event_next
    /// [`paths_iter()`]: struct.Connection.html#method.paths_iter
    /// [`is_draining()`]: struct.Connection.html#method.is_draining
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut out = [0; 512];
    /// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    /// # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    /// # let peer = "127.0.0.1:1234".parse().unwrap();
    /// # let local = socket.local_addr().unwrap();
    /// # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
    /// loop {
    ///     let (write, send_info) = match conn.send_on_path(&mut out, Some(local), Some(peer)) {
    ///         Ok(v) => v,
    ///
    ///         Err(quiche::Error::Done) => {
    ///             // Done writing.
    ///             break;
    ///         },
    ///
    ///         Err(e) => {
    ///             // An error occurred, handle it.
    ///             break;
    ///         },
    ///     };
    ///
    ///     socket.send_to(&out[..write], &send_info.to).unwrap();
    /// }
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn send_on_path(
        &mut self, out: &mut [u8], from: Option<SocketAddr>,
        to: Option<SocketAddr>,
    ) -> Result<(usize, SendInfo)> {
        if out.is_empty() {
            return Err(Error::BufferTooShort);
        }

        if self.is_closed() || self.is_draining() {
            return Err(Error::Done);
        }

        let now = time::Instant::now();

        if self.local_error.is_none() {
            self.do_handshake(now)?;
        }

        // Forwarding the error value here could confuse
        // applications, as they may not expect getting a `recv()`
        // error when calling `send()`.
        //
        // We simply fall-through to sending packets, which should
        // take care of terminating the connection as needed.
        let _ = self.process_undecrypted_0rtt_packets();

        // There's no point in trying to send a packet if the Initial secrets
        // have not been derived yet, so return early.
        if !self.derived_initial_secrets {
            return Err(Error::Done);
        }

        let mut has_initial = false;

        let mut done = 0;

        // Limit output packet size to respect the sender and receiver's
        // maximum UDP payload size limit.
        let mut left = cmp::min(out.len(), self.max_send_udp_payload_size());

        let send_pid = match (from, to) {
            (Some(f), Some(t)) => self
                .paths
                .path_id_from_addrs(&(f, t))
                .ok_or(Error::InvalidState)?,

            _ => self.get_send_path_id(from, to)?,
        };

        let send_path = self.paths.get_mut(send_pid)?;

        // Update max datagram size to allow path MTU discovery probe to be sent.
        if send_path.pmtud.get_probe_status() {
            let size = if self.handshake_confirmed || self.handshake_done_sent {
                send_path.pmtud.get_probe_size()
            } else {
                send_path.pmtud.get_current()
            };

            send_path.recovery.pmtud_update_max_datagram_size(size);

            left = cmp::min(out.len(), send_path.recovery.max_datagram_size());
        }

        // Limit data sent by the server based on the amount of data received
        // from the client before its address is validated.
        if !send_path.verified_peer_address && self.is_server {
            left = cmp::min(left, send_path.max_send_bytes);
        }

        // Generate coalesced packets.
        while left > 0 {
            let (ty, written) = match self.send_single(
                &mut out[done..done + left],
                send_pid,
                has_initial,
                now,
            ) {
                Ok(v) => v,

                Err(Error::BufferTooShort) | Err(Error::Done) => break,

                Err(e) => return Err(e),
            };

            done += written;
            left -= written;

            match ty {
                packet::Type::Initial => has_initial = true,

                // No more packets can be coalesced after a 1-RTT.
                packet::Type::Short => break,

                _ => (),
            };

            // When sending multiple PTO probes, don't coalesce them together,
            // so they are sent on separate UDP datagrams.
            if let Ok(epoch) = ty.to_epoch() {
                if self.paths.get_mut(send_pid)?.recovery.loss_probes(epoch) > 0 {
                    break;
                }
            }

            // Don't coalesce packets that must go on different paths.
            if !(from.is_some() && to.is_some()) &&
                self.get_send_path_id(from, to)? != send_pid
            {
                break;
            }
        }

        if done == 0 {
            self.last_tx_data = self.tx_data;

            return Err(Error::Done);
        }

        // Pad UDP datagram if it contains a QUIC Initial packet.
        #[cfg(not(feature = "fuzzing"))]
        if has_initial && left > 0 && done < MIN_CLIENT_INITIAL_LEN {
            let pad_len = cmp::min(left, MIN_CLIENT_INITIAL_LEN - done);

            // Fill padding area with null bytes, to avoid leaking information
            // in case the application reuses the packet buffer.
            out[done..done + pad_len].fill(0);

            done += pad_len;
        }

        let send_path = self.paths.get(send_pid)?;

        let info = SendInfo {
            from: send_path.local_addr(),
            to: send_path.peer_addr(),

            at: send_path.recovery.get_packet_send_time(),
        };

        Ok((done, info))
    }

    fn send_single(
        &mut self, out: &mut [u8], send_pid: usize, has_initial: bool,
        now: time::Instant,
    ) -> Result<(packet::Type, usize)> {
        if out.is_empty() {
            return Err(Error::BufferTooShort);
        }

        if self.is_draining() {
            return Err(Error::Done);
        }

        let is_closing = self.local_error.is_some();

        let out_len = out.len();

        let mut b = octets::OctetsMut::with_slice(out);

        let pkt_type = self.write_pkt_type(send_pid)?;

        let max_dgram_len = if !self.dgram_send_queue.is_empty() {
            self.dgram_max_writable_len()
        } else {
            None
        };

        let epoch = pkt_type.to_epoch()?;
        let pkt_space = &mut self.pkt_num_spaces[epoch];

        // Process lost frames. There might be several paths having lost frames.
        for (_, p) in self.paths.iter_mut() {
            for lost in p.recovery.get_lost_frames(epoch) {
                match lost {
                    frame::Frame::CryptoHeader { offset, length } => {
                        pkt_space.crypto_stream.send.retransmit(offset, length);

                        self.stream_retrans_bytes += length as u64;
                        p.stream_retrans_bytes += length as u64;

                        self.retrans_count += 1;
                        p.retrans_count += 1;
                    },

                    frame::Frame::StreamHeader {
                        stream_id,
                        offset,
                        length,
                        fin,
                    } => {
                        let stream = match self.streams.get_mut(stream_id) {
                            Some(v) => v,

                            None => continue,
                        };

                        let was_flushable = stream.is_flushable();

                        let empty_fin = length == 0 && fin;

                        stream.send.retransmit(offset, length);

                        // If the stream is now flushable push it to the
                        // flushable queue, but only if it wasn't already
                        // queued.
                        //
                        // Consider the stream flushable also when we are
                        // sending a zero-length frame that has the fin flag
                        // set.
                        if (stream.is_flushable() || empty_fin) && !was_flushable
                        {
                            let priority_key = Arc::clone(&stream.priority_key);
                            self.streams.insert_flushable(&priority_key);
                        }

                        self.stream_retrans_bytes += length as u64;
                        p.stream_retrans_bytes += length as u64;

                        self.retrans_count += 1;
                        p.retrans_count += 1;
                    },

                    frame::Frame::ACK { .. } => {
                        pkt_space.ack_elicited = true;
                    },

                    frame::Frame::ResetStream {
                        stream_id,
                        error_code,
                        final_size,
                    } =>
                        if self.streams.get(stream_id).is_some() {
                            self.streams
                                .insert_reset(stream_id, error_code, final_size);
                        },

                    // Retransmit HANDSHAKE_DONE only if it hasn't been acked at
                    // least once already.
                    frame::Frame::HandshakeDone if !self.handshake_done_acked => {
                        self.handshake_done_sent = false;
                    },

                    frame::Frame::MaxStreamData { stream_id, .. } => {
                        if self.streams.get(stream_id).is_some() {
                            self.streams.insert_almost_full(stream_id);
                        }
                    },

                    frame::Frame::MaxData { .. } => {
                        self.almost_full = true;
                    },

                    frame::Frame::NewConnectionId { seq_num, .. } => {
                        self.ids.mark_advertise_new_scid_seq(seq_num, true);
                    },

                    frame::Frame::RetireConnectionId { seq_num } => {
                        self.ids.mark_retire_dcid_seq(seq_num, true)?;
                    },

                    frame::Frame::Ping { mtu_probe } if mtu_probe.is_some() => {
                        p.pmtud.pmtu_probe_lost();
                    },

                    _ => (),
                }
            }
        }

        let is_app_limited = self.delivery_rate_check_if_app_limited();
        let n_paths = self.paths.len();
        let path = self.paths.get_mut(send_pid)?;
        let flow_control = &mut self.flow_control;
        let pkt_space = &mut self.pkt_num_spaces[epoch];

        let mut left = if path.pmtud.is_enabled() {
            // Limit output buffer size by estimated path MTU.
            cmp::min(path.pmtud.get_current(), b.cap())
        } else {
            b.cap()
        };

        let pn = pkt_space.next_pkt_num;
        let largest_acked_pkt =
            path.recovery.get_largest_acked_on_epoch(epoch).unwrap_or(0);
        let pn_len = packet::pkt_num_len(pn, largest_acked_pkt);

        // The AEAD overhead at the current encryption level.
        let crypto_overhead = pkt_space.crypto_overhead().ok_or(Error::Done)?;

        let dcid_seq = path.active_dcid_seq.ok_or(Error::OutOfIdentifiers)?;

        let dcid =
            ConnectionId::from_ref(self.ids.get_dcid(dcid_seq)?.cid.as_ref());

        let scid = if let Some(scid_seq) = path.active_scid_seq {
            ConnectionId::from_ref(self.ids.get_scid(scid_seq)?.cid.as_ref())
        } else if pkt_type == packet::Type::Short {
            ConnectionId::default()
        } else {
            return Err(Error::InvalidState);
        };

        let hdr = Header {
            ty: pkt_type,

            version: self.version,

            dcid,
            scid,

            pkt_num: 0,
            pkt_num_len: pn_len,

            // Only clone token for Initial packets, as other packets don't have
            // this field (Retry doesn't count, as it's not encoded as part of
            // this code path).
            token: if pkt_type == packet::Type::Initial {
                self.token.clone()
            } else {
                None
            },

            versions: None,
            key_phase: self.key_phase,
        };

        hdr.to_bytes(&mut b)?;

        let hdr_trace = if log::max_level() == log::LevelFilter::Trace {
            Some(format!("{hdr:?}"))
        } else {
            None
        };

        let hdr_ty = hdr.ty;

        #[cfg(feature = "qlog")]
        let qlog_pkt_hdr = self.qlog.streamer.as_ref().map(|_q| {
            qlog::events::quic::PacketHeader::with_type(
                hdr.ty.to_qlog(),
                Some(pn),
                Some(hdr.version),
                Some(&hdr.scid),
                Some(&hdr.dcid),
            )
        });

        // Calculate the space required for the packet, including the header
        // the payload length, the packet number and the AEAD overhead.
        let mut overhead = b.off() + pn_len + crypto_overhead;

        // We assume that the payload length, which is only present in long
        // header packets, can always be encoded with a 2-byte varint.
        if pkt_type != packet::Type::Short {
            overhead += PAYLOAD_LENGTH_LEN;
        }

        // Make sure we have enough space left for the packet overhead.
        match left.checked_sub(overhead) {
            Some(v) => left = v,

            None => {
                // We can't send more because there isn't enough space available
                // in the output buffer.
                //
                // This usually happens when we try to send a new packet but
                // failed because cwnd is almost full. In such case app_limited
                // is set to false here to make cwnd grow when ACK is received.
                path.recovery.update_app_limited(false);
                return Err(Error::Done);
            },
        }

        // Make sure there is enough space for the minimum payload length.
        if left < PAYLOAD_MIN_LEN {
            path.recovery.update_app_limited(false);
            return Err(Error::Done);
        }

        let mut frames: SmallVec<[frame::Frame; 1]> = SmallVec::new();

        let mut ack_eliciting = false;
        let mut in_flight = false;
        // Foll. flag used to upgrade datagram size, if probe successful
        let mut pmtud_probe = false;
        let mut has_data = false;

        // Whether or not we should explicitly elicit an ACK via PING frame if we
        // implicitly elicit one otherwise.
        let ack_elicit_required = path.recovery.should_elicit_ack(epoch);

        let header_offset = b.off();

        // Reserve space for payload length in advance. Since we don't yet know
        // what the final length will be, we reserve 2 bytes in all cases.
        //
        // Only long header packets have an explicit length field.
        if pkt_type != packet::Type::Short {
            b.skip(PAYLOAD_LENGTH_LEN)?;
        }

        packet::encode_pkt_num(pn, pn_len, &mut b)?;

        let payload_offset = b.off();

        let cwnd_available =
            path.recovery.cwnd_available().saturating_sub(overhead);

        let left_before_packing_ack_frame = left;

        // Create ACK frame.
        //
        // When we need to explicitly elicit an ACK via PING later, go ahead and
        // generate an ACK (if there's anything to ACK) since we're going to
        // send a packet with PING anyways, even if we haven't received anything
        // ACK eliciting.
        if pkt_space.recv_pkt_need_ack.len() > 0 &&
            (pkt_space.ack_elicited || ack_elicit_required) &&
            (!is_closing ||
                (pkt_type == Type::Handshake &&
                    self.local_error
                        .as_ref()
                        .map_or(false, |le| le.is_app))) &&
            path.active()
        {
            let ack_delay = pkt_space.largest_rx_pkt_time.elapsed();

            let ack_delay = ack_delay.as_micros() as u64 /
                2_u64
                    .pow(self.local_transport_params.ack_delay_exponent as u32);

            let frame = frame::Frame::ACK {
                ack_delay,
                ranges: pkt_space.recv_pkt_need_ack.clone(),
                ecn_counts: None, // sending ECN is not supported at this time
            };

            // When a PING frame needs to be sent, avoid sending the ACK if
            // there is not enough cwnd available for both (note that PING
            // frames are always 1 byte, so we just need to check that the
            // ACK's length is lower than cwnd).
            if pkt_space.ack_elicited || frame.wire_len() < cwnd_available {
                // ACK-only packets are not congestion controlled so ACKs must
                // be bundled considering the buffer capacity only, and not the
                // available cwnd.
                if push_frame_to_pkt!(b, frames, frame, left) {
                    pkt_space.ack_elicited = false;
                }
            }
        }

        // Limit output packet size by congestion window size.
        left = cmp::min(
            left,
            // Bytes consumed by ACK frames.
            cwnd_available.saturating_sub(left_before_packing_ack_frame - left),
        );

        let mut challenge_data = None;

        let active_path = self.paths.get_active_mut()?;

        if pkt_type == packet::Type::Short {
            // Create PMTUD probe.
            //
            // In order to send a PMTUD probe the current `left` value, which was
            // already limited by the current PMTU measure, needs to be ignored,
            // but the outgoing packet still needs to be limited by
            // the output buffer size, as well as the congestion
            // window.
            //
            // In addition, the PMTUD probe is only generated when the handshake
            // is confirmed, to avoid interfering with the handshake
            // (e.g. due to the anti-amplification limits).

            let pmtu_probe = active_path.should_send_pmtu_probe(
                self.handshake_confirmed,
                self.handshake_done_sent,
                out_len,
                is_closing,
                frames.is_empty(),
            );

            trace!("{} pmtud probe status {} hs_con={} hs_sent={} cwnd_avail={} out_len={} left={}", self.trace_id, pmtu_probe, self.handshake_confirmed, self.handshake_done_sent,
                    active_path.recovery.cwnd_available(), out_len, left);

            if pmtu_probe {
                trace!(
                    "{} sending pmtud probe pmtu_probe={} next_size={} pmtu={}",
                    self.trace_id,
                    active_path.pmtud.get_probe_size(),
                    active_path.pmtud.get_probe_status(),
                    active_path.pmtud.get_current(),
                );

                left = active_path.pmtud.get_probe_size();

                match left.checked_sub(overhead) {
                    Some(v) => left = v,

                    None => {
                        // We can't send more because there isn't enough space
                        // available in the output buffer.
                        //
                        // This usually happens when we try to send a new packet
                        // but failed because cwnd is almost full.
                        //
                        // In such case app_limited is set to false here to make
                        // cwnd grow when ACK is received.
                        active_path.recovery.update_app_limited(false);
                        return Err(Error::Done);
                    },
                }

                let frame = frame::Frame::Padding {
                    len: active_path.pmtud.get_probe_size() - overhead - 1,
                };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    let frame = frame::Frame::Ping {
                        mtu_probe: Some(active_path.pmtud.get_probe_size()),
                    };

                    if push_frame_to_pkt!(b, frames, frame, left) {
                        ack_eliciting = true;
                        in_flight = true;
                    }
                }

                pmtud_probe = true;
            }

            let path = self.paths.get_mut(send_pid)?;
            // Create PATH_RESPONSE frame if needed.
            // We do not try to ensure that these are really sent.
            while let Some(challenge) = path.pop_received_challenge() {
                let frame = frame::Frame::PathResponse { data: challenge };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    ack_eliciting = true;
                    in_flight = true;
                } else {
                    // If there are other pending PATH_RESPONSE, don't lose them
                    // now.
                    break;
                }
            }

            // Create PATH_CHALLENGE frame if needed.
            if path.validation_requested() {
                // TODO: ensure that data is unique over paths.
                let data = rand::rand_u64().to_be_bytes();

                let frame = frame::Frame::PathChallenge { data };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    // Let's notify the path once we know the packet size.
                    challenge_data = Some(data);

                    ack_eliciting = true;
                    in_flight = true;
                }
            }

            if let Some(key_update) = pkt_space.key_update.as_mut() {
                key_update.update_acked = true;
            }
        }

        let path = self.paths.get_mut(send_pid)?;

        if pkt_type == packet::Type::Short && !is_closing {
            // Create NEW_CONNECTION_ID frames as needed.
            while let Some(seq_num) = self.ids.next_advertise_new_scid_seq() {
                let frame = self.ids.get_new_connection_id_frame_for(seq_num)?;

                if push_frame_to_pkt!(b, frames, frame, left) {
                    self.ids.mark_advertise_new_scid_seq(seq_num, false);

                    ack_eliciting = true;
                    in_flight = true;
                } else {
                    break;
                }
            }
        }

        if pkt_type == packet::Type::Short && !is_closing && path.active() {
            // Create HANDSHAKE_DONE frame.
            // self.should_send_handshake_done() but without the need to borrow
            if self.handshake_completed &&
                !self.handshake_done_sent &&
                self.is_server
            {
                let frame = frame::Frame::HandshakeDone;

                if push_frame_to_pkt!(b, frames, frame, left) {
                    self.handshake_done_sent = true;

                    ack_eliciting = true;
                    in_flight = true;
                }
            }

            // Create MAX_STREAMS_BIDI frame.
            if self.streams.should_update_max_streams_bidi() {
                let frame = frame::Frame::MaxStreamsBidi {
                    max: self.streams.max_streams_bidi_next(),
                };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    self.streams.update_max_streams_bidi();

                    ack_eliciting = true;
                    in_flight = true;
                }
            }

            // Create MAX_STREAMS_UNI frame.
            if self.streams.should_update_max_streams_uni() {
                let frame = frame::Frame::MaxStreamsUni {
                    max: self.streams.max_streams_uni_next(),
                };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    self.streams.update_max_streams_uni();

                    ack_eliciting = true;
                    in_flight = true;
                }
            }

            // Create DATA_BLOCKED frame.
            if let Some(limit) = self.blocked_limit {
                let frame = frame::Frame::DataBlocked { limit };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    self.blocked_limit = None;

                    ack_eliciting = true;
                    in_flight = true;
                }
            }

            // Create MAX_STREAM_DATA frames as needed.
            for stream_id in self.streams.almost_full() {
                let stream = match self.streams.get_mut(stream_id) {
                    Some(v) => v,

                    None => {
                        // The stream doesn't exist anymore, so remove it from
                        // the almost full set.
                        self.streams.remove_almost_full(stream_id);
                        continue;
                    },
                };

                // Autotune the stream window size.
                if let Some(factor) = self.oack_scaling_factor {
                    stream.recv.increase_window_by_factor(factor);
                } else {
                    stream.recv.autotune_window(now, path.recovery.rtt());
                }

                let frame = frame::Frame::MaxStreamData {
                    stream_id,
                    max: stream.recv.max_data_next(),
                };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    let recv_win = stream.recv.window();

                    stream.recv.update_max_data(now);

                    self.streams.remove_almost_full(stream_id);

                    ack_eliciting = true;
                    in_flight = true;

                    // Make sure the connection window always has some
                    // room compared to the stream window.
                    flow_control.ensure_window_lower_bound(
                        (recv_win as f64 * CONNECTION_WINDOW_FACTOR) as u64,
                    );

                    // Also send MAX_DATA when MAX_STREAM_DATA is sent, to avoid a
                    // potential race condition.
                    self.almost_full = true;
                }
            }

            // Create MAX_DATA frame as needed.
            if self.almost_full &&
                flow_control.max_data() < flow_control.max_data_next()
            {
                // Autotune the connection window size.
                if let Some(factor) = self.oack_scaling_factor {
                    flow_control.increase_window_by_factor(factor);
                } else {
                    flow_control.autotune_window(now, path.recovery.rtt());
                }

                let frame = frame::Frame::MaxData {
                    max: flow_control.max_data_next(),
                };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    self.almost_full = false;

                    // Commits the new max_rx_data limit.
                    flow_control.update_max_data(now);

                    ack_eliciting = true;
                    in_flight = true;
                }
            }

            // Create STOP_SENDING frames as needed.
            for (stream_id, error_code) in self
                .streams
                .stopped()
                .map(|(&k, &v)| (k, v))
                .collect::<Vec<(u64, u64)>>()
            {
                let frame = frame::Frame::StopSending {
                    stream_id,
                    error_code,
                };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    self.streams.remove_stopped(stream_id);

                    ack_eliciting = true;
                    in_flight = true;
                }
            }

            // Create RESET_STREAM frames as needed.
            for (stream_id, (error_code, final_size)) in self
                .streams
                .reset()
                .map(|(&k, &v)| (k, v))
                .collect::<Vec<(u64, (u64, u64))>>()
            {
                let frame = frame::Frame::ResetStream {
                    stream_id,
                    error_code,
                    final_size,
                };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    self.streams.remove_reset(stream_id);

                    ack_eliciting = true;
                    in_flight = true;
                }
            }

            // Create STREAM_DATA_BLOCKED frames as needed.
            for (stream_id, limit) in self
                .streams
                .blocked()
                .map(|(&k, &v)| (k, v))
                .collect::<Vec<(u64, u64)>>()
            {
                let frame = frame::Frame::StreamDataBlocked { stream_id, limit };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    self.streams.remove_blocked(stream_id);

                    ack_eliciting = true;
                    in_flight = true;
                }
            }

            // Create RETIRE_CONNECTION_ID frames as needed.
            while let Some(seq_num) = self.ids.next_retire_dcid_seq() {
                // The sequence number specified in a RETIRE_CONNECTION_ID frame
                // MUST NOT refer to the Destination Connection ID field of the
                // packet in which the frame is contained.
                let dcid_seq = path.active_dcid_seq.ok_or(Error::InvalidState)?;

                if seq_num == dcid_seq {
                    continue;
                }

                let frame = frame::Frame::RetireConnectionId { seq_num };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    self.ids.mark_retire_dcid_seq(seq_num, false)?;

                    ack_eliciting = true;
                    in_flight = true;
                } else {
                    break;
                }
            }
        }

        // Create CONNECTION_CLOSE frame. Try to send this only on the active
        // path, unless it is the last one available.
        if path.active() || n_paths == 1 {
            if let Some(conn_err) = self.local_error.as_ref() {
                if conn_err.is_app {
                    // Create ApplicationClose frame.
                    if pkt_type == packet::Type::Short {
                        let frame = frame::Frame::ApplicationClose {
                            error_code: conn_err.error_code,
                            reason: conn_err.reason.clone(),
                        };

                        if push_frame_to_pkt!(b, frames, frame, left) {
                            let pto = path.recovery.pto();
                            self.draining_timer = Some(now + (pto * 3));

                            ack_eliciting = true;
                            in_flight = true;
                        }
                    }
                } else {
                    // Create ConnectionClose frame.
                    let frame = frame::Frame::ConnectionClose {
                        error_code: conn_err.error_code,
                        frame_type: 0,
                        reason: conn_err.reason.clone(),
                    };

                    if push_frame_to_pkt!(b, frames, frame, left) {
                        let pto = path.recovery.pto();
                        self.draining_timer = Some(now + (pto * 3));

                        ack_eliciting = true;
                        in_flight = true;
                    }
                }
            }
        }

        // Create CRYPTO frame.
        if pkt_space.crypto_stream.is_flushable() &&
            left > frame::MAX_CRYPTO_OVERHEAD &&
            !is_closing &&
            path.active()
        {
            let crypto_off = pkt_space.crypto_stream.send.off_front();

            // Encode the frame.
            //
            // Instead of creating a `frame::Frame` object, encode the frame
            // directly into the packet buffer.
            //
            // First we reserve some space in the output buffer for writing the
            // frame header (we assume the length field is always a 2-byte
            // varint as we don't know the value yet).
            //
            // Then we emit the data from the crypto stream's send buffer.
            //
            // Finally we go back and encode the frame header with the now
            // available information.
            let hdr_off = b.off();
            let hdr_len = 1 + // frame type
                octets::varint_len(crypto_off) + // offset
                2; // length, always encode as 2-byte varint

            if let Some(max_len) = left.checked_sub(hdr_len) {
                let (mut crypto_hdr, mut crypto_payload) =
                    b.split_at(hdr_off + hdr_len)?;

                // Write stream data into the packet buffer.
                let (len, _) = pkt_space
                    .crypto_stream
                    .send
                    .emit(&mut crypto_payload.as_mut()[..max_len])?;

                // Encode the frame's header.
                //
                // Due to how `OctetsMut::split_at()` works, `crypto_hdr` starts
                // from the initial offset of `b` (rather than the current
                // offset), so it needs to be advanced to the
                // initial frame offset.
                crypto_hdr.skip(hdr_off)?;

                frame::encode_crypto_header(
                    crypto_off,
                    len as u64,
                    &mut crypto_hdr,
                )?;

                // Advance the packet buffer's offset.
                b.skip(hdr_len + len)?;

                let frame = frame::Frame::CryptoHeader {
                    offset: crypto_off,
                    length: len,
                };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    ack_eliciting = true;
                    in_flight = true;
                    has_data = true;
                }
            }
        }

        // The preference of data-bearing frame to include in a packet
        // is managed by `self.emit_dgram`. However, whether any frames
        // can be sent depends on the state of their buffers. In the case
        // where one type is preferred but its buffer is empty, fall back
        // to the other type in order not to waste this function call.
        let mut dgram_emitted = false;
        let dgrams_to_emit = max_dgram_len.is_some();
        let stream_to_emit = self.streams.has_flushable();

        let mut do_dgram = self.emit_dgram && dgrams_to_emit;
        let do_stream = !self.emit_dgram && stream_to_emit;

        if !do_stream && dgrams_to_emit {
            do_dgram = true;
        }

        // Create DATAGRAM frame.
        if (pkt_type == packet::Type::Short || pkt_type == packet::Type::ZeroRTT) &&
            left > frame::MAX_DGRAM_OVERHEAD &&
            !is_closing &&
            path.active() &&
            do_dgram
        {
            if let Some(max_dgram_payload) = max_dgram_len {
                while let Some(len) = self.dgram_send_queue.peek_front_len() {
                    let hdr_off = b.off();
                    let hdr_len = 1 + // frame type
                        2; // length, always encode as 2-byte varint

                    if (hdr_len + len) <= left {
                        // Front of the queue fits this packet, send it.
                        match self.dgram_send_queue.pop() {
                            Some(data) => {
                                // Encode the frame.
                                //
                                // Instead of creating a `frame::Frame` object,
                                // encode the frame directly into the packet
                                // buffer.
                                //
                                // First we reserve some space in the output
                                // buffer for writing the frame header (we
                                // assume the length field is always a 2-byte
                                // varint as we don't know the value yet).
                                //
                                // Then we emit the data from the DATAGRAM's
                                // buffer.
                                //
                                // Finally we go back and encode the frame
                                // header with the now available information.
                                let (mut dgram_hdr, mut dgram_payload) =
                                    b.split_at(hdr_off + hdr_len)?;

                                dgram_payload.as_mut()[..len]
                                    .copy_from_slice(&data);

                                // Encode the frame's header.
                                //
                                // Due to how `OctetsMut::split_at()` works,
                                // `dgram_hdr` starts from the initial offset
                                // of `b` (rather than the current offset), so
                                // it needs to be advanced to the initial frame
                                // offset.
                                dgram_hdr.skip(hdr_off)?;

                                frame::encode_dgram_header(
                                    len as u64,
                                    &mut dgram_hdr,
                                )?;

                                // Advance the packet buffer's offset.
                                b.skip(hdr_len + len)?;

                                let frame =
                                    frame::Frame::DatagramHeader { length: len };

                                if push_frame_to_pkt!(b, frames, frame, left) {
                                    ack_eliciting = true;
                                    in_flight = true;
                                    dgram_emitted = true;
                                }
                            },

                            None => continue,
                        };
                    } else if len > max_dgram_payload {
                        // This dgram frame will never fit. Let's purge it.
                        self.dgram_send_queue.pop();
                    } else {
                        break;
                    }
                }
            }
        }

        // Create a single STREAM frame for the first stream that is flushable.
        if (pkt_type == packet::Type::Short || pkt_type == packet::Type::ZeroRTT) &&
            left > frame::MAX_STREAM_OVERHEAD &&
            !is_closing &&
            path.active() &&
            !dgram_emitted
        {
            while let Some(priority_key) = self.streams.peek_flushable() {
                let stream_id = priority_key.id;
                let stream = match self.streams.get_mut(stream_id) {
                    // Avoid sending frames for streams that were already stopped.
                    //
                    // This might happen if stream data was buffered but not yet
                    // flushed on the wire when a STOP_SENDING frame is received.
                    Some(v) if !v.send.is_stopped() => v,
                    _ => {
                        self.streams.remove_flushable(&priority_key);
                        continue;
                    },
                };

                let stream_off = stream.send.off_front();

                // Encode the frame.
                //
                // Instead of creating a `frame::Frame` object, encode the frame
                // directly into the packet buffer.
                //
                // First we reserve some space in the output buffer for writing
                // the frame header (we assume the length field is always a
                // 2-byte varint as we don't know the value yet).
                //
                // Then we emit the data from the stream's send buffer.
                //
                // Finally we go back and encode the frame header with the now
                // available information.
                let hdr_off = b.off();
                let hdr_len = 1 + // frame type
                    octets::varint_len(stream_id) + // stream_id
                    octets::varint_len(stream_off) + // offset
                    2; // length, always encode as 2-byte varint

                let max_len = match left.checked_sub(hdr_len) {
                    Some(v) => v,
                    None => {
                        let priority_key = Arc::clone(&stream.priority_key);
                        self.streams.remove_flushable(&priority_key);

                        continue;
                    },
                };

                let (mut stream_hdr, mut stream_payload) =
                    b.split_at(hdr_off + hdr_len)?;

                // Write stream data into the packet buffer.
                let (len, fin) =
                    stream.send.emit(&mut stream_payload.as_mut()[..max_len])?;

                // Encode the frame's header.
                //
                // Due to how `OctetsMut::split_at()` works, `stream_hdr` starts
                // from the initial offset of `b` (rather than the current
                // offset), so it needs to be advanced to the initial frame
                // offset.
                stream_hdr.skip(hdr_off)?;

                frame::encode_stream_header(
                    stream_id,
                    stream_off,
                    len as u64,
                    fin,
                    &mut stream_hdr,
                )?;

                // Advance the packet buffer's offset.
                b.skip(hdr_len + len)?;

                let frame = frame::Frame::StreamHeader {
                    stream_id,
                    offset: stream_off,
                    length: len,
                    fin,
                };

                if push_frame_to_pkt!(b, frames, frame, left) {
                    ack_eliciting = true;
                    in_flight = true;
                    has_data = true;
                }

                let priority_key = Arc::clone(&stream.priority_key);
                // If the stream is no longer flushable, remove it from the queue
                if !stream.is_flushable() {
                    self.streams.remove_flushable(&priority_key);
                } else if stream.incremental {
                    // Shuffle the incremental stream to the back of the
                    // queue.
                    self.streams.remove_flushable(&priority_key);
                    self.streams.insert_flushable(&priority_key);
                }

                #[cfg(feature = "fuzzing")]
                // Coalesce STREAM frames when fuzzing.
                if left > frame::MAX_STREAM_OVERHEAD {
                    continue;
                }

                break;
            }
        }

        // Alternate trying to send DATAGRAMs next time.
        self.emit_dgram = !dgram_emitted;

        // If no other ack-eliciting frame is sent, include a PING frame
        // - if PTO probe needed; OR
        // - if we've sent too many non ack-eliciting packets without having
        // sent an ACK eliciting one; OR
        // - the application requested an ack-eliciting frame be sent.
        if (ack_elicit_required || path.needs_ack_eliciting) &&
            !ack_eliciting &&
            left >= 1 &&
            !is_closing
        {
            let frame = frame::Frame::Ping { mtu_probe: None };

            if push_frame_to_pkt!(b, frames, frame, left) {
                ack_eliciting = true;
                in_flight = true;
            }
        }

        if ack_eliciting && !pmtud_probe {
            path.needs_ack_eliciting = false;
            path.recovery.ping_sent(epoch);
        }

        if frames.is_empty() {
            // When we reach this point we are not able to write more, so set
            // app_limited to false.
            path.recovery.update_app_limited(false);
            return Err(Error::Done);
        }

        // When coalescing a 1-RTT packet, we can't add padding in the UDP
        // datagram, so use PADDING frames instead.
        //
        // This is only needed if
        // 1) an Initial packet has already been written to the UDP datagram,
        // as Initial always requires padding.
        //
        // 2) this is a probing packet towards an unvalidated peer address.
        if (has_initial || !path.validated()) &&
            (pkt_type == packet::Type::Short) &&
            left >= 1 || (pkt_type == packet::Type::Initial && left >= 1)
        {
            info!("ADD PADDING TO THE PACKET");
            let frame = frame::Frame::Padding { len: left };

            if push_frame_to_pkt!(b, frames, frame, left) {
                in_flight = true;
            }
        }

        // Pad payload so that it's always at least 4 bytes.
        if b.off() - payload_offset < PAYLOAD_MIN_LEN {
            let payload_len = b.off() - payload_offset;

            let frame = frame::Frame::Padding {
                len: PAYLOAD_MIN_LEN - payload_len,
            };

            #[allow(unused_assignments)]
            if push_frame_to_pkt!(b, frames, frame, left) {
                in_flight = true;
            }
        }

        let payload_len = b.off() - payload_offset;

        // Fill in payload length.
        if pkt_type != packet::Type::Short {
            let len = pn_len + payload_len + crypto_overhead;

            let (_, mut payload_with_len) = b.split_at(header_offset)?;
            payload_with_len
                .put_varint_with_len(len as u64, PAYLOAD_LENGTH_LEN)?;
        }

        trace!(
            "{} tx pkt {} len={} pn={} {}",
            self.trace_id,
            hdr_trace.unwrap_or_default(),
            payload_len,
            pn,
            AddrTupleFmt(path.local_addr(), path.peer_addr())
        );

        #[cfg(feature = "qlog")]
        let mut qlog_frames: SmallVec<
            [qlog::events::quic::QuicFrame; 1],
        > = SmallVec::with_capacity(frames.len());

        for frame in &mut frames {
            trace!("{} tx frm {:?}", self.trace_id, frame);

            qlog_with_type!(QLOG_PACKET_TX, self.qlog, _q, {
                qlog_frames.push(frame.to_qlog());
            });
        }

        qlog_with_type!(QLOG_PACKET_TX, self.qlog, q, {
            if let Some(header) = qlog_pkt_hdr {
                // Qlog packet raw info described at
                // https://datatracker.ietf.org/doc/html/draft-ietf-quic-qlog-main-schema-00#section-5.1
                //
                // `length` includes packet headers and trailers (AEAD tag).
                let length = payload_len + payload_offset + crypto_overhead;
                let qlog_raw_info = RawInfo {
                    length: Some(length as u64),
                    payload_length: Some(payload_len as u64),
                    data: None,
                };

                let send_at_time =
                    now.duration_since(q.start_time()).as_secs_f32() * 1000.0;

                let ev_data =
                    EventData::PacketSent(qlog::events::quic::PacketSent {
                        header,
                        frames: Some(qlog_frames),
                        is_coalesced: None,
                        retry_token: None,
                        stateless_reset_token: None,
                        supported_versions: None,
                        raw: Some(qlog_raw_info),
                        datagram_id: None,
                        send_at_time: Some(send_at_time),
                        trigger: None,
                    });

                q.add_event_data_with_instant(ev_data, now).ok();
            }
        });

        let aead = match pkt_space.crypto_seal {
            Some(ref v) => v,
            None => return Err(Error::InvalidState),
        };

        let written = packet::encrypt_pkt(
            &mut b,
            pn,
            pn_len,
            payload_len,
            payload_offset,
            None,
            aead,
        )?;

        let sent_pkt = recovery::Sent {
            pkt_num: pn,
            frames,
            time_sent: now,
            time_acked: None,
            time_lost: None,
            size: if ack_eliciting { written } else { 0 },
            ack_eliciting,
            in_flight,
            delivered: 0,
            delivered_time: now,
            first_sent_time: now,
            is_app_limited: false,
            tx_in_flight: 0,
            lost: 0,
            has_data,
            pmtud: pmtud_probe,
        };

        if in_flight && is_app_limited {
            path.recovery.delivery_rate_update_app_limited(true);
        }

        pkt_space.next_pkt_num += 1;

        let handshake_status = recovery::HandshakeStatus {
            has_handshake_keys: self.pkt_num_spaces[packet::Epoch::Handshake]
                .has_keys(),
            peer_verified_address: self.peer_verified_initial_address,
            completed: self.handshake_completed,
        };

        path.recovery.on_packet_sent(
            sent_pkt,
            epoch,
            handshake_status,
            now,
            &self.trace_id,
        );

        qlog_with_type!(QLOG_METRICS, self.qlog, q, {
            if let Some(ev_data) = path.recovery.maybe_qlog() {
                q.add_event_data_with_instant(ev_data, now).ok();
            }
        });

        // Record sent packet size if we probe the path.
        if let Some(data) = challenge_data {
            path.add_challenge_sent(data, written, now);
        }

        self.sent_count += 1;
        self.sent_bytes += written as u64;
        path.sent_count += 1;
        path.sent_bytes += written as u64;

        if self.dgram_send_queue.byte_size() > path.recovery.cwnd_available() {
            path.recovery.update_app_limited(false);
        }

        path.max_send_bytes = path.max_send_bytes.saturating_sub(written);

        // On the client, drop initial state after sending an Handshake packet.
        if !self.is_server && hdr_ty == packet::Type::Handshake {
            self.drop_epoch_state(packet::Epoch::Initial, now);
        }

        // (Re)start the idle timer if we are sending the first ack-eliciting
        // packet since last receiving a packet.
        if ack_eliciting && !self.ack_eliciting_sent {
            if let Some(idle_timeout) = self.idle_timeout() {
                self.idle_timer = Some(now + idle_timeout);
            }
        }

        if ack_eliciting {
            self.ack_eliciting_sent = true;
        }

        let active_path = self.paths.get_active_mut()?;
        if active_path.pmtud.is_enabled() {
            active_path
                .recovery
                .pmtud_update_max_datagram_size(active_path.pmtud.get_current());
        }

        Ok((pkt_type, written))
    }

    /// Returns the size of the send quantum, in bytes.
    ///
    /// This represents the maximum size of a packet burst as determined by the
    /// congestion control algorithm in use.
    ///
    /// Applications can, for example, use it in conjunction with segmentation
    /// offloading mechanisms as the maximum limit for outgoing aggregates of
    /// multiple packets.
    #[inline]
    pub fn send_quantum(&self) -> usize {
        match self.paths.get_active() {
            Ok(p) => p.recovery.send_quantum(),
            _ => 0,
        }
    }

    /// Returns the size of the send quantum over the given 4-tuple, in bytes.
    ///
    /// This represents the maximum size of a packet burst as determined by the
    /// congestion control algorithm in use.
    ///
    /// Applications can, for example, use it in conjunction with segmentation
    /// offloading mechanisms as the maximum limit for outgoing aggregates of
    /// multiple packets.
    ///
    /// If the (`local_addr`, peer_addr`) 4-tuple relates to a non-existing
    /// path, this method returns 0.
    pub fn send_quantum_on_path(
        &self, local_addr: SocketAddr, peer_addr: SocketAddr,
    ) -> usize {
        self.paths
            .path_id_from_addrs(&(local_addr, peer_addr))
            .and_then(|pid| self.paths.get(pid).ok())
            .map(|path| path.recovery.send_quantum())
            .unwrap_or(0)
    }

    /// Reads contiguous data from a stream into the provided slice.
    ///
    /// The slice must be sized by the caller and will be populated up to its
    /// capacity.
    ///
    /// On success the amount of bytes read and a flag indicating the fin state
    /// is returned as a tuple, or [`Done`] if there is no data to read.
    ///
    /// Reading data from a stream may trigger queueing of control messages
    /// (e.g. MAX_STREAM_DATA). [`send()`] should be called after reading.
    ///
    /// [`Done`]: enum.Error.html#variant.Done
    /// [`send()`]: struct.Connection.html#method.send
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut buf = [0; 512];
    /// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    /// # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    /// # let peer = "127.0.0.1:1234".parse().unwrap();
    /// # let local = socket.local_addr().unwrap();
    /// # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
    /// # let stream_id = 0;
    /// while let Ok((read, fin)) = conn.stream_recv(stream_id, &mut buf) {
    ///     println!("Got {} bytes on stream {}", read, stream_id);
    /// }
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn stream_recv(
        &mut self, stream_id: u64, out: &mut [u8],
    ) -> Result<(usize, bool)> {
        // We can't read on our own unidirectional streams.
        if !stream::is_bidi(stream_id) &&
            stream::is_local(stream_id, self.is_server)
        {
            return Err(Error::InvalidStreamState(stream_id));
        }

        let stream = self
            .streams
            .get_mut(stream_id)
            .ok_or(Error::InvalidStreamState(stream_id))?;

        if !stream.is_readable() {
            return Err(Error::Done);
        }

        let local = stream.local;
        let priority_key = Arc::clone(&stream.priority_key);

        #[cfg(feature = "qlog")]
        let offset = stream.recv.off_front();

        let (read, fin) = match stream.recv.emit(out) {
            Ok(v) => v,

            Err(e) => {
                // Collect the stream if it is now complete. This can happen if
                // we got a `StreamReset` error which will now be propagated to
                // the application, so we don't need to keep the stream's state
                // anymore.
                if stream.is_complete() {
                    self.streams.collect(stream_id, local);
                }

                self.streams.remove_readable(&priority_key);
                return Err(e);
            },
        };

        self.flow_control.add_consumed(read as u64);

        let readable = stream.is_readable();

        let complete = stream.is_complete();

        if stream.recv.almost_full() {
            self.streams.insert_almost_full(stream_id);
        }

        if !readable {
            self.streams.remove_readable(&priority_key);
        }

        if complete {
            self.streams.collect(stream_id, local);
        }

        qlog_with_type!(QLOG_DATA_MV, self.qlog, q, {
            let ev_data = EventData::DataMoved(qlog::events::quic::DataMoved {
                stream_id: Some(stream_id),
                offset: Some(offset),
                length: Some(read as u64),
                from: Some(DataRecipient::Transport),
                to: Some(DataRecipient::Application),
                raw: None,
            });

            let now = time::Instant::now();
            q.add_event_data_with_instant(ev_data, now).ok();
        });

        if self.should_update_max_data() {
            self.almost_full = true;
        }

        if priority_key.incremental && readable {
            // Shuffle the incremental stream to the back of the queue.
            self.streams.remove_readable(&priority_key);
            self.streams.insert_readable(&priority_key);
        }

        Ok((read, fin))
    }

    /// Writes data to a stream.
    ///
    /// On success the number of bytes written is returned, or [`Done`] if no
    /// data was written (e.g. because the stream has no capacity).
    ///
    /// Applications can provide a 0-length buffer with the fin flag set to
    /// true. This will lead to a 0-length FIN STREAM frame being sent at the
    /// latest offset. The `Ok(0)` value is only returned when the application
    /// provided a 0-length buffer.
    ///
    /// In addition, if the peer has signalled that it doesn't want to receive
    /// any more data from this stream by sending the `STOP_SENDING` frame, the
    /// [`StreamStopped`] error will be returned instead of any data.
    ///
    /// Note that in order to avoid buffering an infinite amount of data in the
    /// stream's send buffer, streams are only allowed to buffer outgoing data
    /// up to the amount that the peer allows it to send (that is, up to the
    /// stream's outgoing flow control capacity).
    ///
    /// This means that the number of written bytes returned can be lower than
    /// the length of the input buffer when the stream doesn't have enough
    /// capacity for the operation to complete. The application should retry the
    /// operation once the stream is reported as writable again.
    ///
    /// Applications should call this method only after the handshake is
    /// completed (whenever [`is_established()`] returns `true`) or during
    /// early data if enabled (whenever [`is_in_early_data()`] returns `true`).
    ///
    /// [`Done`]: enum.Error.html#variant.Done
    /// [`StreamStopped`]: enum.Error.html#variant.StreamStopped
    /// [`is_established()`]: struct.Connection.html#method.is_established
    /// [`is_in_early_data()`]: struct.Connection.html#method.is_in_early_data
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut buf = [0; 512];
    /// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    /// # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    /// # let peer = "127.0.0.1:1234".parse().unwrap();
    /// # let local = "127.0.0.1:4321".parse().unwrap();
    /// # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
    /// # let stream_id = 0;
    /// conn.stream_send(stream_id, b"hello", true)?;
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn stream_send(
        &mut self, stream_id: u64, buf: &[u8], fin: bool,
    ) -> Result<usize> {
        // We can't write on the peer's unidirectional streams.
        if !stream::is_bidi(stream_id) &&
            !stream::is_local(stream_id, self.is_server)
        {
            return Err(Error::InvalidStreamState(stream_id));
        }

        // Mark the connection as blocked if the connection-level flow control
        // limit doesn't let us buffer all the data.
        //
        // Note that this is separate from "send capacity" as that also takes
        // congestion control into consideration.
        if self.max_tx_data - self.tx_data < buf.len() as u64 {
            self.blocked_limit = Some(self.max_tx_data);
        }

        let cap = self.tx_cap;

        // Get existing stream or create a new one.
        let stream = self.get_or_create_stream(stream_id, true)?;

        #[cfg(feature = "qlog")]
        let offset = stream.send.off_back();

        let was_writable = stream.is_writable();

        let was_flushable = stream.is_flushable();

        let priority_key = Arc::clone(&stream.priority_key);

        // Truncate the input buffer based on the connection's send capacity if
        // necessary.
        //
        // When the cap is zero, the method returns Ok(0) *only* when the passed
        // buffer is empty. We return Error::Done otherwise.
        if cap == 0 && !buf.is_empty() {
            if was_writable {
                // When `stream_writable_next()` returns a stream, the writable
                // mark is removed, but because the stream is blocked by the
                // connection-level send capacity it won't be marked as writable
                // again once the capacity increases.
                //
                // Since the stream is writable already, mark it here instead.
                self.streams.insert_writable(&priority_key);
            }

            return Err(Error::Done);
        }

        let (buf, fin, blocked_by_cap) = if cap < buf.len() {
            (&buf[..cap], false, true)
        } else {
            (buf, fin, false)
        };

        let sent = match stream.send.write(buf, fin) {
            Ok(v) => v,

            Err(e) => {
                self.streams.remove_writable(&priority_key);
                return Err(e);
            },
        };

        let incremental = stream.incremental;
        let priority_key = Arc::clone(&stream.priority_key);

        let flushable = stream.is_flushable();

        let writable = stream.is_writable();

        let empty_fin = buf.is_empty() && fin;

        if sent < buf.len() {
            let max_off = stream.send.max_off();

            if stream.send.blocked_at() != Some(max_off) {
                stream.send.update_blocked_at(Some(max_off));
                self.streams.insert_blocked(stream_id, max_off);
            }
        } else {
            stream.send.update_blocked_at(None);
            self.streams.remove_blocked(stream_id);
        }

        // If the stream is now flushable push it to the flushable queue, but
        // only if it wasn't already queued.
        //
        // Consider the stream flushable also when we are sending a zero-length
        // frame that has the fin flag set.
        if (flushable || empty_fin) && !was_flushable {
            self.streams.insert_flushable(&priority_key);
        }

        if !writable {
            self.streams.remove_writable(&priority_key);
        } else if was_writable && blocked_by_cap {
            // When `stream_writable_next()` returns a stream, the writable
            // mark is removed, but because the stream is blocked by the
            // connection-level send capacity it won't be marked as writable
            // again once the capacity increases.
            //
            // Since the stream is writable already, mark it here instead.
            self.streams.insert_writable(&priority_key);
        }

        self.tx_cap -= sent;

        self.tx_data += sent as u64;

        self.tx_buffered += sent;

        qlog_with_type!(QLOG_DATA_MV, self.qlog, q, {
            let ev_data = EventData::DataMoved(qlog::events::quic::DataMoved {
                stream_id: Some(stream_id),
                offset: Some(offset),
                length: Some(sent as u64),
                from: Some(DataRecipient::Application),
                to: Some(DataRecipient::Transport),
                raw: None,
            });

            let now = time::Instant::now();
            q.add_event_data_with_instant(ev_data, now).ok();
        });

        if sent == 0 && !buf.is_empty() {
            return Err(Error::Done);
        }

        if incremental && writable {
            // Shuffle the incremental stream to the back of the queue.
            self.streams.remove_writable(&priority_key);
            self.streams.insert_writable(&priority_key);
        }

        Ok(sent)
    }

    /// Sets the priority for a stream.
    ///
    /// A stream's priority determines the order in which stream data is sent
    /// on the wire (streams with lower priority are sent first). Streams are
    /// created with a default priority of `127`.
    ///
    /// The target stream is created if it did not exist before calling this
    /// method.
    pub fn stream_priority(
        &mut self, stream_id: u64, urgency: u8, incremental: bool,
    ) -> Result<()> {
        // Get existing stream or create a new one, but if the stream
        // has already been closed and collected, ignore the prioritization.
        let stream = match self.get_or_create_stream(stream_id, true) {
            Ok(v) => v,

            Err(Error::Done) => return Ok(()),

            Err(e) => return Err(e),
        };

        if stream.urgency == urgency && stream.incremental == incremental {
            return Ok(());
        }

        stream.urgency = urgency;
        stream.incremental = incremental;

        let new_priority_key = Arc::new(StreamPriorityKey {
            urgency: stream.urgency,
            incremental: stream.incremental,
            id: stream_id,
            ..Default::default()
        });

        let old_priority_key =
            std::mem::replace(&mut stream.priority_key, new_priority_key.clone());

        self.streams
            .update_priority(&old_priority_key, &new_priority_key);

        Ok(())
    }

    /// Shuts down reading or writing from/to the specified stream.
    ///
    /// When the `direction` argument is set to [`Shutdown::Read`], outstanding
    /// data in the stream's receive buffer is dropped, and no additional data
    /// is added to it. Data received after calling this method is still
    /// validated and acked but not stored, and [`stream_recv()`] will not
    /// return it to the application. In addition, a `STOP_SENDING` frame will
    /// be sent to the peer to signal it to stop sending data.
    ///
    /// When the `direction` argument is set to [`Shutdown::Write`], outstanding
    /// data in the stream's send buffer is dropped, and no additional data is
    /// added to it. Data passed to [`stream_send()`] after calling this method
    /// will be ignored. In addition, a `RESET_STREAM` frame will be sent to the
    /// peer to signal the reset.
    ///
    /// Locally-initiated unidirectional streams can only be closed in the
    /// [`Shutdown::Write`] direction. Remotely-initiated unidirectional streams
    /// can only be closed in the [`Shutdown::Read`] direction. Using an
    /// incorrect direction will return [`InvalidStreamState`].
    ///
    /// [`Shutdown::Read`]: enum.Shutdown.html#variant.Read
    /// [`Shutdown::Write`]: enum.Shutdown.html#variant.Write
    /// [`stream_recv()`]: struct.Connection.html#method.stream_recv
    /// [`stream_send()`]: struct.Connection.html#method.stream_send
    /// [`InvalidStreamState`]: enum.Error.html#variant.InvalidStreamState
    pub fn stream_shutdown(
        &mut self, stream_id: u64, direction: Shutdown, err: u64,
    ) -> Result<()> {
        // Don't try to stop a local unidirectional stream.
        if direction == Shutdown::Read &&
            stream::is_local(stream_id, self.is_server) &&
            !stream::is_bidi(stream_id)
        {
            return Err(Error::InvalidStreamState(stream_id));
        }

        // Dont' try to reset a remote unidirectional stream.
        if direction == Shutdown::Write &&
            !stream::is_local(stream_id, self.is_server) &&
            !stream::is_bidi(stream_id)
        {
            return Err(Error::InvalidStreamState(stream_id));
        }

        // Get existing stream.
        let stream = self.streams.get_mut(stream_id).ok_or(Error::Done)?;

        let priority_key = Arc::clone(&stream.priority_key);

        match direction {
            Shutdown::Read => {
                stream.recv.shutdown()?;

                if !stream.recv.is_fin() {
                    self.streams.insert_stopped(stream_id, err);
                }

                // Once shutdown, the stream is guaranteed to be non-readable.
                self.streams.remove_readable(&priority_key);

                self.stopped_stream_local_count =
                    self.stopped_stream_local_count.saturating_add(1);
            },

            Shutdown::Write => {
                let (final_size, unsent) = stream.send.shutdown()?;

                // Claw back some flow control allowance from data that was
                // buffered but not actually sent before the stream was reset.
                self.tx_data = self.tx_data.saturating_sub(unsent);

                self.tx_buffered =
                    self.tx_buffered.saturating_sub(unsent as usize);

                // Update send capacity.
                self.update_tx_cap();

                self.streams.insert_reset(stream_id, err, final_size);

                // Once shutdown, the stream is guaranteed to be non-writable.
                self.streams.remove_writable(&priority_key);

                self.reset_stream_local_count =
                    self.reset_stream_local_count.saturating_add(1);
            },
        }

        Ok(())
    }

    /// Returns the stream's send capacity in bytes.
    ///
    /// If the specified stream doesn't exist (including when it has already
    /// been completed and closed), the [`InvalidStreamState`] error will be
    /// returned.
    ///
    /// In addition, if the peer has signalled that it doesn't want to receive
    /// any more data from this stream by sending the `STOP_SENDING` frame, the
    /// [`StreamStopped`] error will be returned.
    ///
    /// [`InvalidStreamState`]: enum.Error.html#variant.InvalidStreamState
    /// [`StreamStopped`]: enum.Error.html#variant.StreamStopped
    #[inline]
    pub fn stream_capacity(&self, stream_id: u64) -> Result<usize> {
        if let Some(stream) = self.streams.get(stream_id) {
            let cap = cmp::min(self.tx_cap, stream.send.cap()?);
            return Ok(cap);
        };

        Err(Error::InvalidStreamState(stream_id))
    }

    /// Returns the next stream that has data to read.
    ///
    /// Note that once returned by this method, a stream ID will not be returned
    /// again until it is "re-armed".
    ///
    /// The application will need to read all of the pending data on the stream,
    /// and new data has to be received before the stream is reported again.
    ///
    /// This is unlike the [`readable()`] method, that returns the same list of
    /// readable streams when called multiple times in succession.
    ///
    /// [`readable()`]: struct.Connection.html#method.readable
    pub fn stream_readable_next(&mut self) -> Option<u64> {
        let priority_key = self.streams.readable.front().clone_pointer()?;

        self.streams.remove_readable(&priority_key);

        Some(priority_key.id)
    }

    /// Returns true if the stream has data that can be read.
    pub fn stream_readable(&self, stream_id: u64) -> bool {
        let stream = match self.streams.get(stream_id) {
            Some(v) => v,

            None => return false,
        };

        stream.is_readable()
    }

    /// Returns the next stream that can be written to.
    ///
    /// Note that once returned by this method, a stream ID will not be returned
    /// again until it is "re-armed".
    ///
    /// This is unlike the [`writable()`] method, that returns the same list of
    /// writable streams when called multiple times in succession. It is not
    /// advised to use both `stream_writable_next()` and [`writable()`] on the
    /// same connection, as it may lead to unexpected results.
    ///
    /// The [`stream_writable()`] method can also be used to fine-tune when a
    /// stream is reported as writable again.
    ///
    /// [`stream_writable()`]: struct.Connection.html#method.stream_writable
    /// [`writable()`]: struct.Connection.html#method.writable
    pub fn stream_writable_next(&mut self) -> Option<u64> {
        // If there is not enough connection-level send capacity, none of the
        // streams are writable.
        if self.tx_cap == 0 {
            return None;
        }

        let mut cursor = self.streams.writable.front();

        while let Some(priority_key) = cursor.clone_pointer() {
            if let Some(stream) = self.streams.get(priority_key.id) {
                let cap = match stream.send.cap() {
                    Ok(v) => v,

                    // Return the stream to the application immediately if it's
                    // stopped.
                    Err(_) =>
                        return {
                            self.streams.remove_writable(&priority_key);

                            Some(priority_key.id)
                        },
                };

                if cmp::min(self.tx_cap, cap) >= stream.send_lowat {
                    self.streams.remove_writable(&priority_key);
                    return Some(priority_key.id);
                }
            }

            cursor.move_next();
        }

        None
    }

    /// Returns true if the stream has enough send capacity.
    ///
    /// When `len` more bytes can be buffered into the given stream's send
    /// buffer, `true` will be returned, `false` otherwise.
    ///
    /// In the latter case, if the additional data can't be buffered due to
    /// flow control limits, the peer will also be notified, and a "low send
    /// watermark" will be set for the stream, such that it is not going to be
    /// reported as writable again by [`stream_writable_next()`] until its send
    /// capacity reaches `len`.
    ///
    /// If the specified stream doesn't exist (including when it has already
    /// been completed and closed), the [`InvalidStreamState`] error will be
    /// returned.
    ///
    /// In addition, if the peer has signalled that it doesn't want to receive
    /// any more data from this stream by sending the `STOP_SENDING` frame, the
    /// [`StreamStopped`] error will be returned.
    ///
    /// [`stream_writable_next()`]: struct.Connection.html#method.stream_writable_next
    /// [`InvalidStreamState`]: enum.Error.html#variant.InvalidStreamState
    /// [`StreamStopped`]: enum.Error.html#variant.StreamStopped
    #[inline]
    pub fn stream_writable(
        &mut self, stream_id: u64, len: usize,
    ) -> Result<bool> {
        if self.stream_capacity(stream_id)? >= len {
            return Ok(true);
        }

        let stream = match self.streams.get_mut(stream_id) {
            Some(v) => v,

            None => return Err(Error::InvalidStreamState(stream_id)),
        };

        stream.send_lowat = cmp::max(1, len);

        let is_writable = stream.is_writable();

        let priority_key = Arc::clone(&stream.priority_key);

        if self.max_tx_data - self.tx_data < len as u64 {
            self.blocked_limit = Some(self.max_tx_data);
        }

        if stream.send.cap()? < len {
            let max_off = stream.send.max_off();
            if stream.send.blocked_at() != Some(max_off) {
                stream.send.update_blocked_at(Some(max_off));
                self.streams.insert_blocked(stream_id, max_off);
            }
        } else if is_writable {
            // When `stream_writable_next()` returns a stream, the writable
            // mark is removed, but because the stream is blocked by the
            // connection-level send capacity it won't be marked as writable
            // again once the capacity increases.
            //
            // Since the stream is writable already, mark it here instead.
            self.streams.insert_writable(&priority_key);
        }

        Ok(false)
    }

    /// Returns true if all the data has been read from the specified stream.
    ///
    /// This instructs the application that all the data received from the
    /// peer on the stream has been read, and there won't be anymore in the
    /// future.
    ///
    /// Basically this returns true when the peer either set the `fin` flag
    /// for the stream, or sent `RESET_STREAM`.
    #[inline]
    pub fn stream_finished(&self, stream_id: u64) -> bool {
        let stream = match self.streams.get(stream_id) {
            Some(v) => v,

            None => return true,
        };

        stream.recv.is_fin()
    }

    /// Returns the number of bidirectional streams that can be created
    /// before the peer's stream count limit is reached.
    ///
    /// This can be useful to know if it's possible to create a bidirectional
    /// stream without trying it first.
    #[inline]
    pub fn peer_streams_left_bidi(&self) -> u64 {
        self.streams.peer_streams_left_bidi()
    }

    /// Returns the number of unidirectional streams that can be created
    /// before the peer's stream count limit is reached.
    ///
    /// This can be useful to know if it's possible to create a unidirectional
    /// stream without trying it first.
    #[inline]
    pub fn peer_streams_left_uni(&self) -> u64 {
        self.streams.peer_streams_left_uni()
    }

    /// Returns an iterator over streams that have outstanding data to read.
    ///
    /// Note that the iterator will only include streams that were readable at
    /// the time the iterator itself was created (i.e. when `readable()` was
    /// called). To account for newly readable streams, the iterator needs to
    /// be created again.
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut buf = [0; 512];
    /// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    /// # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    /// # let peer = "127.0.0.1:1234".parse().unwrap();
    /// # let local = socket.local_addr().unwrap();
    /// # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
    /// // Iterate over readable streams.
    /// for stream_id in conn.readable() {
    ///     // Stream is readable, read until there's no more data.
    ///     while let Ok((read, fin)) = conn.stream_recv(stream_id, &mut buf) {
    ///         println!("Got {} bytes on stream {}", read, stream_id);
    ///     }
    /// }
    /// # Ok::<(), quiche::Error>(())
    /// ```
    #[inline]
    pub fn readable(&self) -> StreamIter {
        self.streams.readable()
    }

    /// Returns an iterator over streams that can be written in priority order.
    ///
    /// The priority order is based on RFC 9218 scheduling recommendations.
    /// Stream priority can be controlled using [`stream_priority()`]. In order
    /// to support fairness requirements, each time this method is called,
    /// internal state is updated. Therefore the iterator ordering can change
    /// between calls, even if no streams were added or removed.
    ///
    /// A "writable" stream is a stream that has enough flow control capacity to
    /// send data to the peer. To avoid buffering an infinite amount of data,
    /// streams are only allowed to buffer outgoing data up to the amount that
    /// the peer allows to send.
    ///
    /// Note that the iterator will only include streams that were writable at
    /// the time the iterator itself was created (i.e. when `writable()` was
    /// called). To account for newly writable streams, the iterator needs to be
    /// created again.
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut buf = [0; 512];
    /// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    /// # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    /// # let local = socket.local_addr().unwrap();
    /// # let peer = "127.0.0.1:1234".parse().unwrap();
    /// # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
    /// // Iterate over writable streams.
    /// for stream_id in conn.writable() {
    ///     // Stream is writable, write some data.
    ///     if let Ok(written) = conn.stream_send(stream_id, &buf, false) {
    ///         println!("Written {} bytes on stream {}", written, stream_id);
    ///     }
    /// }
    /// # Ok::<(), quiche::Error>(())
    /// ```
    /// [`stream_priority()`]: struct.Connection.html#method.stream_priority
    #[inline]
    pub fn writable(&self) -> StreamIter {
        // If there is not enough connection-level send capacity, none of the
        // streams are writable, so return an empty iterator.
        if self.tx_cap == 0 {
            return StreamIter::default();
        }

        self.streams.writable()
    }

    /// Returns the maximum possible size of egress UDP payloads.
    ///
    /// This is the maximum size of UDP payloads that can be sent, and depends
    /// on both the configured maximum send payload size of the local endpoint
    /// (as configured with [`set_max_send_udp_payload_size()`]), as well as
    /// the transport parameter advertised by the remote peer.
    ///
    /// Note that this value can change during the lifetime of the connection,
    /// but should remain stable across consecutive calls to [`send()`].
    ///
    /// [`set_max_send_udp_payload_size()`]:
    ///     struct.Config.html#method.set_max_send_udp_payload_size
    /// [`send()`]: struct.Connection.html#method.send
    pub fn max_send_udp_payload_size(&self) -> usize {
        let max_datagram_size = self
            .paths
            .get_active()
            .ok()
            .map(|p| p.recovery.max_datagram_size());

        if let Some(max_datagram_size) = max_datagram_size {
            if self.is_established() {
                // We cap the maximum packet size to 16KB or so, so that it can be
                // always encoded with a 2-byte varint.
                return cmp::min(16383, max_datagram_size);
            }
        }

        // Allow for 1200 bytes (minimum QUIC packet size) during the
        // handshake.
        MIN_CLIENT_INITIAL_LEN
    }

    /// Schedule an ack-eliciting packet on the active path.
    ///
    /// QUIC packets might not contain ack-eliciting frames during normal
    /// operating conditions. If the packet would already contain
    /// ack-eliciting frames, this method does not change any behavior.
    /// However, if the packet would not ordinarily contain ack-eliciting
    /// frames, this method ensures that a PING frame sent.
    ///
    /// Calling this method multiple times before [`send()`] has no effect.
    ///
    /// [`send()`]: struct.Connection.html#method.send
    pub fn send_ack_eliciting(&mut self) -> Result<()> {
        if self.is_closed() || self.is_draining() {
            return Ok(());
        }
        self.paths.get_active_mut()?.needs_ack_eliciting = true;
        Ok(())
    }

    /// Schedule an ack-eliciting packet on the specified path.
    ///
    /// See [`send_ack_eliciting()`] for more detail. [`InvalidState`] is
    /// returned if there is no record of the path.
    ///
    /// [`send_ack_eliciting()`]: struct.Connection.html#method.send_ack_eliciting
    /// [`InvalidState`]: enum.Error.html#variant.InvalidState
    pub fn send_ack_eliciting_on_path(
        &mut self, local: SocketAddr, peer: SocketAddr,
    ) -> Result<()> {
        if self.is_closed() || self.is_draining() {
            return Ok(());
        }
        let path_id = self
            .paths
            .path_id_from_addrs(&(local, peer))
            .ok_or(Error::InvalidState)?;
        self.paths.get_mut(path_id)?.needs_ack_eliciting = true;
        Ok(())
    }

    /// Reads the first received DATAGRAM.
    ///
    /// On success the DATAGRAM's data is returned along with its size.
    ///
    /// [`Done`] is returned if there is no data to read.
    ///
    /// [`BufferTooShort`] is returned if the provided buffer is too small for
    /// the DATAGRAM.
    ///
    /// [`Done`]: enum.Error.html#variant.Done
    /// [`BufferTooShort`]: enum.Error.html#variant.BufferTooShort
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut buf = [0; 512];
    /// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    /// # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    /// # let peer = "127.0.0.1:1234".parse().unwrap();
    /// # let local = socket.local_addr().unwrap();
    /// # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
    /// let mut dgram_buf = [0; 512];
    /// while let Ok((len)) = conn.dgram_recv(&mut dgram_buf) {
    ///     println!("Got {} bytes of DATAGRAM", len);
    /// }
    /// # Ok::<(), quiche::Error>(())
    /// ```
    #[inline]
    pub fn dgram_recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        match self.dgram_recv_queue.pop() {
            Some(d) => {
                if d.len() > buf.len() {
                    return Err(Error::BufferTooShort);
                }

                buf[..d.len()].copy_from_slice(&d);
                Ok(d.len())
            },

            None => Err(Error::Done),
        }
    }

    /// Reads the first received DATAGRAM.
    ///
    /// This is the same as [`dgram_recv()`] but returns the DATAGRAM as a
    /// `Vec<u8>` instead of copying into the provided buffer.
    ///
    /// [`dgram_recv()`]: struct.Connection.html#method.dgram_recv
    #[inline]
    pub fn dgram_recv_vec(&mut self) -> Result<Vec<u8>> {
        match self.dgram_recv_queue.pop() {
            Some(d) => Ok(d),

            None => Err(Error::Done),
        }
    }

    /// Reads the first received DATAGRAM without removing it from the queue.
    ///
    /// On success the DATAGRAM's data is returned along with the actual number
    /// of bytes peeked. The requested length cannot exceed the DATAGRAM's
    /// actual length.
    ///
    /// [`Done`] is returned if there is no data to read.
    ///
    /// [`BufferTooShort`] is returned if the provided buffer is smaller the
    /// number of bytes to peek.
    ///
    /// [`Done`]: enum.Error.html#variant.Done
    /// [`BufferTooShort`]: enum.Error.html#variant.BufferTooShort
    #[inline]
    pub fn dgram_recv_peek(&self, buf: &mut [u8], len: usize) -> Result<usize> {
        self.dgram_recv_queue.peek_front_bytes(buf, len)
    }

    /// Returns the length of the first stored DATAGRAM.
    #[inline]
    pub fn dgram_recv_front_len(&self) -> Option<usize> {
        self.dgram_recv_queue.peek_front_len()
    }

    /// Returns the number of items in the DATAGRAM receive queue.
    #[inline]
    pub fn dgram_recv_queue_len(&self) -> usize {
        self.dgram_recv_queue.len()
    }

    /// Returns the total size of all items in the DATAGRAM receive queue.
    #[inline]
    pub fn dgram_recv_queue_byte_size(&self) -> usize {
        self.dgram_recv_queue.byte_size()
    }

    /// Returns the number of items in the DATAGRAM send queue.
    #[inline]
    pub fn dgram_send_queue_len(&self) -> usize {
        self.dgram_send_queue.len()
    }

    /// Returns the total size of all items in the DATAGRAM send queue.
    #[inline]
    pub fn dgram_send_queue_byte_size(&self) -> usize {
        self.dgram_send_queue.byte_size()
    }

    /// Returns whether or not the DATAGRAM send queue is full.
    #[inline]
    pub fn is_dgram_send_queue_full(&self) -> bool {
        self.dgram_send_queue.is_full()
    }

    /// Returns whether or not the DATAGRAM recv queue is full.
    #[inline]
    pub fn is_dgram_recv_queue_full(&self) -> bool {
        self.dgram_recv_queue.is_full()
    }

    /// Sends data in a DATAGRAM frame.
    ///
    /// [`Done`] is returned if no data was written.
    /// [`InvalidState`] is returned if the peer does not support DATAGRAM.
    /// [`BufferTooShort`] is returned if the DATAGRAM frame length is larger
    /// than peer's supported DATAGRAM frame length. Use
    /// [`dgram_max_writable_len()`] to get the largest supported DATAGRAM
    /// frame length.
    ///
    /// Note that there is no flow control of DATAGRAM frames, so in order to
    /// avoid buffering an infinite amount of frames we apply an internal
    /// limit.
    ///
    /// [`Done`]: enum.Error.html#variant.Done
    /// [`InvalidState`]: enum.Error.html#variant.InvalidState
    /// [`BufferTooShort`]: enum.Error.html#variant.BufferTooShort
    /// [`dgram_max_writable_len()`]:
    /// struct.Connection.html#method.dgram_max_writable_len
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut buf = [0; 512];
    /// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    /// # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    /// # let peer = "127.0.0.1:1234".parse().unwrap();
    /// # let local = socket.local_addr().unwrap();
    /// # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
    /// conn.dgram_send(b"hello")?;
    /// # Ok::<(), quiche::Error>(())
    /// ```
    pub fn dgram_send(&mut self, buf: &[u8]) -> Result<()> {
        let max_payload_len = match self.dgram_max_writable_len() {
            Some(v) => v,

            None => return Err(Error::InvalidState),
        };

        if buf.len() > max_payload_len {
            return Err(Error::BufferTooShort);
        }

        self.dgram_send_queue.push(buf.to_vec())?;

        let active_path = self.paths.get_active_mut()?;

        if self.dgram_send_queue.byte_size() >
            active_path.recovery.cwnd_available()
        {
            active_path.recovery.update_app_limited(false);
        }

        Ok(())
    }

    /// Sends data in a DATAGRAM frame.
    ///
    /// This is the same as [`dgram_send()`] but takes a `Vec<u8>` instead of
    /// a slice.
    ///
    /// [`dgram_send()`]: struct.Connection.html#method.dgram_send
    pub fn dgram_send_vec(&mut self, buf: Vec<u8>) -> Result<()> {
        let max_payload_len = match self.dgram_max_writable_len() {
            Some(v) => v,

            None => return Err(Error::InvalidState),
        };

        if buf.len() > max_payload_len {
            return Err(Error::BufferTooShort);
        }

        self.dgram_send_queue.push(buf)?;

        let active_path = self.paths.get_active_mut()?;

        if self.dgram_send_queue.byte_size() >
            active_path.recovery.cwnd_available()
        {
            active_path.recovery.update_app_limited(false);
        }

        Ok(())
    }

    /// Purges queued outgoing DATAGRAMs matching the predicate.
    ///
    /// In other words, remove all elements `e` such that `f(&e)` returns true.
    ///
    /// ## Examples:
    /// ```no_run
    /// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    /// # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    /// # let peer = "127.0.0.1:1234".parse().unwrap();
    /// # let local = socket.local_addr().unwrap();
    /// # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
    /// conn.dgram_send(b"hello")?;
    /// conn.dgram_purge_outgoing(&|d: &[u8]| -> bool { d[0] == 0 });
    /// # Ok::<(), quiche::Error>(())
    /// ```
    #[inline]
    pub fn dgram_purge_outgoing<F: Fn(&[u8]) -> bool>(&mut self, f: F) {
        self.dgram_send_queue.purge(f);
    }

    /// Returns the maximum DATAGRAM payload that can be sent.
    ///
    /// [`None`] is returned if the peer hasn't advertised a maximum DATAGRAM
    /// frame size.
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut buf = [0; 512];
    /// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    /// # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    /// # let peer = "127.0.0.1:1234".parse().unwrap();
    /// # let local = socket.local_addr().unwrap();
    /// # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
    /// if let Some(payload_size) = conn.dgram_max_writable_len() {
    ///     if payload_size > 5 {
    ///         conn.dgram_send(b"hello")?;
    ///     }
    /// }
    /// # Ok::<(), quiche::Error>(())
    /// ```
    #[inline]
    pub fn dgram_max_writable_len(&self) -> Option<usize> {
        match self.peer_transport_params.max_datagram_frame_size {
            None => None,
            Some(peer_frame_len) => {
                let dcid = self.destination_id();
                // Start from the maximum packet size...
                let mut max_len = self.max_send_udp_payload_size();
                // ...subtract the Short packet header overhead...
                // (1 byte of pkt_len + len of dcid)
                max_len = max_len.saturating_sub(1 + dcid.len());
                // ...subtract the packet number (max len)...
                max_len = max_len.saturating_sub(packet::MAX_PKT_NUM_LEN);
                // ...subtract the crypto overhead...
                max_len = max_len.saturating_sub(
                    self.pkt_num_spaces[packet::Epoch::Application]
                        .crypto_overhead()?,
                );
                // ...clamp to what peer can support...
                max_len = cmp::min(peer_frame_len as usize, max_len);
                // ...subtract frame overhead, checked for underflow.
                // (1 byte of frame type + len of length )
                max_len.checked_sub(1 + frame::MAX_DGRAM_OVERHEAD)
            },
        }
    }

    fn dgram_enabled(&self) -> bool {
        self.local_transport_params
            .max_datagram_frame_size
            .is_some()
    }

    /// Returns when the next timeout event will occur.
    ///
    /// Once the timeout Instant has been reached, the [`on_timeout()`] method
    /// should be called. A timeout of `None` means that the timer should be
    /// disarmed.
    ///
    /// [`on_timeout()`]: struct.Connection.html#method.on_timeout
    pub fn timeout_instant(&self) -> Option<time::Instant> {
        if self.is_closed() {
            return None;
        }

        if self.is_draining() {
            // Draining timer takes precedence over all other timers. If it is
            // set it means the connection is closing so there's no point in
            // processing the other timers.
            self.draining_timer
        } else {
            // Use the lowest timer value (i.e. "sooner") among idle and loss
            // detection timers. If they are both unset (i.e. `None`) then the
            // result is `None`, but if at least one of them is set then a
            // `Some(...)` value is returned.
            let path_timer = self
                .paths
                .iter()
                .filter_map(|(_, p)| p.recovery.loss_detection_timer())
                .min();

            let key_update_timer = self.pkt_num_spaces
                [packet::Epoch::Application]
                .key_update
                .as_ref()
                .map(|key_update| key_update.timer);

            let timers = [self.idle_timer, path_timer, key_update_timer];

            timers.iter().filter_map(|&x| x).min()
        }
    }

    /// Returns the amount of time until the next timeout event.
    ///
    /// Once the given duration has elapsed, the [`on_timeout()`] method should
    /// be called. A timeout of `None` means that the timer should be disarmed.
    ///
    /// [`on_timeout()`]: struct.Connection.html#method.on_timeout
    pub fn timeout(&self) -> Option<time::Duration> {
        self.timeout_instant().map(|timeout| {
            let now = time::Instant::now();

            if timeout <= now {
                time::Duration::ZERO
            } else {
                timeout.duration_since(now)
            }
        })
    }

    /// Processes a timeout event.
    ///
    /// If no timeout has occurred it does nothing.
    pub fn on_timeout(&mut self) {
        let now = time::Instant::now();

        if let Some(draining_timer) = self.draining_timer {
            if draining_timer <= now {
                trace!("{} draining timeout expired", self.trace_id);

                self.mark_closed();
            }

            // Draining timer takes precedence over all other timers. If it is
            // set it means the connection is closing so there's no point in
            // processing the other timers.
            return;
        }

        if let Some(timer) = self.idle_timer {
            if timer <= now {
                trace!("{} idle timeout expired", self.trace_id);

                self.mark_closed();
                self.timed_out = true;
                return;
            }
        }

        if let Some(timer) = self.pkt_num_spaces[packet::Epoch::Application]
            .key_update
            .as_ref()
            .map(|key_update| key_update.timer)
        {
            if timer <= now {
                // Discard previous key once key update timer expired.
                let _ = self.pkt_num_spaces[packet::Epoch::Application]
                    .key_update
                    .take();
            }
        }

        let handshake_status = self.handshake_status();

        for (_, p) in self.paths.iter_mut() {
            if let Some(timer) = p.recovery.loss_detection_timer() {
                if timer <= now {
                    trace!("{} loss detection timeout expired", self.trace_id);

                    let (lost_packets, lost_bytes) = p.on_loss_detection_timeout(
                        handshake_status,
                        now,
                        self.is_server,
                        &self.trace_id,
                    );

                    self.lost_count += lost_packets;
                    self.lost_bytes += lost_bytes as u64;

                    qlog_with_type!(QLOG_METRICS, self.qlog, q, {
                        if let Some(ev_data) = p.recovery.maybe_qlog() {
                            q.add_event_data_with_instant(ev_data, now).ok();
                        }
                    });
                }
            }
        }

        // Notify timeout events to the application.
        self.paths.notify_failed_validations();

        // If the active path failed, try to find a new candidate.
        if self.paths.get_active_path_id().is_err() {
            match self.paths.find_candidate_path() {
                Some(pid) => {
                    if self.set_active_path(pid, now).is_err() {
                        // The connection cannot continue.
                        self.mark_closed();
                    }
                },

                // The connection cannot continue.
                None => {
                    self.mark_closed();
                },
            }
        }
    }

    /// Requests the stack to perform path validation of the proposed 4-tuple.
    ///
    /// Probing new paths requires spare Connection IDs at both the host and the
    /// peer sides. If it is not the case, it raises an [`OutOfIdentifiers`].
    ///
    /// The probing of new addresses can only be done by the client. The server
    /// can only probe network paths that were previously advertised by
    /// [`PathEvent::New`]. If the server tries to probe such an unseen network
    /// path, this call raises an [`InvalidState`].
    ///
    /// The caller might also want to probe an existing path. In such case, it
    /// triggers a PATH_CHALLENGE frame, but it does not require spare CIDs.
    ///
    /// A server always probes a new path it observes. Calling this method is
    /// hence not required to validate a new path. However, a server can still
    /// request an additional path validation of the proposed 4-tuple.
    ///
    /// Calling this method several times before calling [`send()`] or
    /// [`send_on_path()`] results in a single probe being generated. An
    /// application wanting to send multiple in-flight probes must call this
    /// method again after having sent packets.
    ///
    /// Returns the Destination Connection ID sequence number associated to that
    /// path.
    ///
    /// [`PathEvent::New`]: enum.PathEvent.html#variant.New
    /// [`OutOfIdentifiers`]: enum.Error.html#OutOfIdentifiers
    /// [`InvalidState`]: enum.Error.html#InvalidState
    /// [`send()`]: struct.Connection.html#method.send
    /// [`send_on_path()`]: struct.Connection.html#method.send_on_path
    pub fn probe_path(
        &mut self, local_addr: SocketAddr, peer_addr: SocketAddr,
    ) -> Result<u64> {
        // We may want to probe an existing path.
        let pid = match self.paths.path_id_from_addrs(&(local_addr, peer_addr)) {
            Some(pid) => pid,
            None => self.create_path_on_client(local_addr, peer_addr)?,
        };

        let path = self.paths.get_mut(pid)?;
        path.request_validation();

        path.active_dcid_seq.ok_or(Error::InvalidState)
    }

    /// Migrates the connection to a new local address `local_addr`.
    ///
    /// The behavior is similar to [`migrate()`], with the nuance that the
    /// connection only changes the local address, but not the peer one.
    ///
    /// See [`migrate()`] for the full specification of this method.
    ///
    /// [`migrate()`]: struct.Connection.html#method.migrate
    pub fn migrate_source(&mut self, local_addr: SocketAddr) -> Result<u64> {
        let peer_addr = self.paths.get_active()?.peer_addr();
        self.migrate(local_addr, peer_addr)
    }

    /// Migrates the connection over the given network path between `local_addr`
    /// and `peer_addr`.
    ///
    /// Connection migration can only be initiated by the client. Calling this
    /// method as a server returns [`InvalidState`].
    ///
    /// To initiate voluntary migration, there should be enough Connection IDs
    /// at both sides. If this requirement is not satisfied, this call returns
    /// [`OutOfIdentifiers`].
    ///
    /// Returns the Destination Connection ID associated to that migrated path.
    ///
    /// [`OutOfIdentifiers`]: enum.Error.html#OutOfIdentifiers
    /// [`InvalidState`]: enum.Error.html#InvalidState
    pub fn migrate(
        &mut self, local_addr: SocketAddr, peer_addr: SocketAddr,
    ) -> Result<u64> {
        if self.is_server {
            return Err(Error::InvalidState);
        }

        // If the path already exists, mark it as the active one.
        let (pid, dcid_seq) = if let Some(pid) =
            self.paths.path_id_from_addrs(&(local_addr, peer_addr))
        {
            let path = self.paths.get_mut(pid)?;

            // If it is already active, do nothing.
            if path.active() {
                return path.active_dcid_seq.ok_or(Error::OutOfIdentifiers);
            }

            // Ensures that a Source Connection ID has been dedicated to this
            // path, or a free one is available. This is only required if the
            // host uses non-zero length Source Connection IDs.
            if !self.ids.zero_length_scid() &&
                path.active_scid_seq.is_none() &&
                self.ids.available_scids() == 0
            {
                return Err(Error::OutOfIdentifiers);
            }

            // Ensures that the migrated path has a Destination Connection ID.
            let dcid_seq = if let Some(dcid_seq) = path.active_dcid_seq {
                dcid_seq
            } else {
                let dcid_seq = self
                    .ids
                    .lowest_available_dcid_seq()
                    .ok_or(Error::OutOfIdentifiers)?;

                self.ids.link_dcid_to_path_id(dcid_seq, pid)?;
                path.active_dcid_seq = Some(dcid_seq);

                dcid_seq
            };

            (pid, dcid_seq)
        } else {
            let pid = self.create_path_on_client(local_addr, peer_addr)?;

            let dcid_seq = self
                .paths
                .get(pid)?
                .active_dcid_seq
                .ok_or(Error::InvalidState)?;

            (pid, dcid_seq)
        };

        // Change the active path.
        self.set_active_path(pid, time::Instant::now())?;

        Ok(dcid_seq)
    }

    /// Provides additional source Connection IDs that the peer can use to reach
    /// this host.
    ///
    /// This triggers sending NEW_CONNECTION_ID frames if the provided Source
    /// Connection ID is not already present. In the case the caller tries to
    /// reuse a Connection ID with a different reset token, this raises an
    /// `InvalidState`.
    ///
    /// At any time, the peer cannot have more Destination Connection IDs than
    /// the maximum number of active Connection IDs it negotiated. In such case
    /// (i.e., when [`scids_left()`] returns 0), if the host agrees to
    /// request the removal of previous connection IDs, it sets the
    /// `retire_if_needed` parameter. Otherwise, an [`IdLimit`] is returned.
    ///
    /// Note that setting `retire_if_needed` does not prevent this function from
    /// returning an [`IdLimit`] in the case the caller wants to retire still
    /// unannounced Connection IDs.
    ///
    /// The caller is responsible from ensuring that the provided `scid` is not
    /// repeated several times over the connection. quiche ensures that as long
    /// as the provided Connection ID is still in use (i.e., not retired), it
    /// does not assign a different sequence number.
    ///
    /// Note that if the host uses zero-length Source Connection IDs, it cannot
    /// advertise Source Connection IDs and calling this method returns an
    /// [`InvalidState`].
    ///
    /// Returns the sequence number associated to the provided Connection ID.
    ///
    /// [`scids_left()`]: struct.Connection.html#method.scids_left
    /// [`IdLimit`]: enum.Error.html#IdLimit
    /// [`InvalidState`]: enum.Error.html#InvalidState
    pub fn new_scid(
        &mut self, scid: &ConnectionId, reset_token: u128, retire_if_needed: bool,
    ) -> Result<u64> {
        self.ids.new_scid(
            scid.to_vec().into(),
            Some(reset_token),
            true,
            None,
            retire_if_needed,
        )
    }

    /// Returns the number of source Connection IDs that are active. This is
    /// only meaningful if the host uses non-zero length Source Connection IDs.
    pub fn active_scids(&self) -> usize {
        self.ids.active_source_cids()
    }

    /// Returns the number of source Connection IDs that should be provided
    /// to the peer without exceeding the limit it advertised.
    ///
    /// This will automatically limit the number of Connection IDs to the
    /// minimum between the locally configured active connection ID limit,
    /// and the one sent by the peer.
    ///
    /// To obtain the maximum possible value allowed by the peer an application
    /// can instead inspect the [`peer_active_conn_id_limit`] value.
    ///
    /// [`peer_active_conn_id_limit`]: struct.Stats.html#structfield.peer_active_conn_id_limit
    #[inline]
    pub fn scids_left(&self) -> usize {
        let max_active_source_cids = cmp::min(
            self.peer_transport_params.active_conn_id_limit,
            self.local_transport_params.active_conn_id_limit,
        ) as usize;

        max_active_source_cids - self.active_scids()
    }

    /// Requests the retirement of the destination Connection ID used by the
    /// host to reach its peer.
    ///
    /// This triggers sending RETIRE_CONNECTION_ID frames.
    ///
    /// If the application tries to retire a non-existing Destination Connection
    /// ID sequence number, or if it uses zero-length Destination Connection ID,
    /// this method returns an [`InvalidState`].
    ///
    /// At any time, the host must have at least one Destination ID. If the
    /// application tries to retire the last one, or if the caller tries to
    /// retire the destination Connection ID used by the current active path
    /// while having neither spare Destination Connection IDs nor validated
    /// network paths, this method returns an [`OutOfIdentifiers`]. This
    /// behavior prevents the caller from stalling the connection due to the
    /// lack of validated path to send non-probing packets.
    ///
    /// [`InvalidState`]: enum.Error.html#InvalidState
    /// [`OutOfIdentifiers`]: enum.Error.html#OutOfIdentifiers
    pub fn retire_dcid(&mut self, dcid_seq: u64) -> Result<()> {
        if self.ids.zero_length_dcid() {
            return Err(Error::InvalidState);
        }

        let active_path_dcid_seq = self
            .paths
            .get_active()?
            .active_dcid_seq
            .ok_or(Error::InvalidState)?;

        let active_path_id = self.paths.get_active_path_id()?;

        if active_path_dcid_seq == dcid_seq &&
            self.ids.lowest_available_dcid_seq().is_none() &&
            !self
                .paths
                .iter()
                .any(|(pid, p)| pid != active_path_id && p.usable())
        {
            return Err(Error::OutOfIdentifiers);
        }

        if let Some(pid) = self.ids.retire_dcid(dcid_seq)? {
            // The retired Destination CID was associated to a given path. Let's
            // find an available DCID to associate to that path.
            let path = self.paths.get_mut(pid)?;
            let dcid_seq = self.ids.lowest_available_dcid_seq();

            if let Some(dcid_seq) = dcid_seq {
                self.ids.link_dcid_to_path_id(dcid_seq, pid)?;
            }

            path.active_dcid_seq = dcid_seq;
        }

        Ok(())
    }

    /// Processes path-specific events.
    ///
    /// On success it returns a [`PathEvent`], or `None` when there are no
    /// events to report. Please refer to [`PathEvent`] for the exhaustive event
    /// list.
    ///
    /// Note that all events are edge-triggered, meaning that once reported they
    /// will not be reported again by calling this method again, until the event
    /// is re-armed.
    ///
    /// [`PathEvent`]: enum.PathEvent.html
    pub fn path_event_next(&mut self) -> Option<PathEvent> {
        self.paths.pop_event()
    }

    /// Returns the number of source Connection IDs that are retired.
    pub fn retired_scids(&self) -> usize {
        self.ids.retired_source_cids()
    }

    /// Returns a source `ConnectionId` that has been retired.
    ///
    /// On success it returns a [`ConnectionId`], or `None` when there are no
    /// more retired connection IDs.
    ///
    /// [`ConnectionId`]: struct.ConnectionId.html
    pub fn retired_scid_next(&mut self) -> Option<ConnectionId<'static>> {
        self.ids.pop_retired_scid()
    }

    /// Returns the number of spare Destination Connection IDs, i.e.,
    /// Destination Connection IDs that are still unused.
    ///
    /// Note that this function returns 0 if the host uses zero length
    /// Destination Connection IDs.
    pub fn available_dcids(&self) -> usize {
        self.ids.available_dcids()
    }

    /// Returns an iterator over destination `SockAddr`s whose association
    /// with `from` forms a known QUIC path on which packets can be sent to.
    ///
    /// This function is typically used in combination with [`send_on_path()`].
    ///
    /// Note that the iterator includes all the possible combination of
    /// destination `SockAddr`s, even those whose sending is not required now.
    /// In other words, this is another way for the application to recall from
    /// past [`PathEvent::New`] events.
    ///
    /// [`PathEvent::New`]: enum.PathEvent.html#variant.New
    /// [`send_on_path()`]: struct.Connection.html#method.send_on_path
    ///
    /// ## Examples:
    ///
    /// ```no_run
    /// # let mut out = [0; 512];
    /// # let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    /// # let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
    /// # let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    /// # let local = socket.local_addr().unwrap();
    /// # let peer = "127.0.0.1:1234".parse().unwrap();
    /// # let mut conn = quiche::accept(&scid, None, local, peer, &mut config)?;
    /// // Iterate over possible destinations for the given local `SockAddr`.
    /// for dest in conn.paths_iter(local) {
    ///     loop {
    ///         let (write, send_info) =
    ///             match conn.send_on_path(&mut out, Some(local), Some(dest)) {
    ///                 Ok(v) => v,
    ///
    ///                 Err(quiche::Error::Done) => {
    ///                     // Done writing for this destination.
    ///                     break;
    ///                 },
    ///
    ///                 Err(e) => {
    ///                     // An error occurred, handle it.
    ///                     break;
    ///                 },
    ///             };
    ///
    ///         socket.send_to(&out[..write], &send_info.to).unwrap();
    ///     }
    /// }
    /// # Ok::<(), quiche::Error>(())
    /// ```
    #[inline]
    pub fn paths_iter(&self, from: SocketAddr) -> SocketAddrIter {
        // Instead of trying to identify whether packets will be sent on the
        // given 4-tuple, simply filter paths that cannot be used.
        SocketAddrIter {
            sockaddrs: self
                .paths
                .iter()
                .filter(|(_, p)| p.active_dcid_seq.is_some())
                .filter(|(_, p)| p.usable() || p.probing_required())
                .filter(|(_, p)| p.local_addr() == from)
                .map(|(_, p)| p.peer_addr())
                .collect(),

            index: 0,
        }
    }

    /// Closes the connection with the given error and reason.
    ///
    /// The `app` parameter specifies whether an application close should be
    /// sent to the peer. Otherwise a normal connection close is sent.
    ///
    /// If `app` is true but the connection is not in a state that is safe to
    /// send an application error (not established nor in early data), in
    /// accordance with [RFC
    /// 9000](https://www.rfc-editor.org/rfc/rfc9000.html#section-10.2.3-3), the
    /// error code is changed to APPLICATION_ERROR and the reason phrase is
    /// cleared.
    ///
    /// Returns [`Done`] if the connection had already been closed.
    ///
    /// Note that the connection will not be closed immediately. An application
    /// should continue calling the [`recv()`], [`send()`], [`timeout()`] and
    /// [`on_timeout()`] methods as normal, until the [`is_closed()`] method
    /// returns `true`.
    ///
    /// [`Done`]: enum.Error.html#variant.Done
    /// [`recv()`]: struct.Connection.html#method.recv
    /// [`send()`]: struct.Connection.html#method.send
    /// [`timeout()`]: struct.Connection.html#method.timeout
    /// [`on_timeout()`]: struct.Connection.html#method.on_timeout
    /// [`is_closed()`]: struct.Connection.html#method.is_closed
    pub fn close(&mut self, app: bool, err: u64, reason: &[u8]) -> Result<()> {
        if self.is_closed() || self.is_draining() {
            return Err(Error::Done);
        }

        if self.local_error.is_some() {
            return Err(Error::Done);
        }

        let is_safe_to_send_app_data =
            self.is_established() || self.is_in_early_data();

        if app && !is_safe_to_send_app_data {
            // Clear error information.
            self.local_error = Some(ConnectionError {
                is_app: false,
                error_code: 0x0c,
                reason: vec![],
            });
        } else {
            self.local_error = Some(ConnectionError {
                is_app: app,
                error_code: err,
                reason: reason.to_vec(),
            });
        }

        // When no packet was successfully processed close connection immediately.
        if self.recv_count == 0 {
            self.mark_closed();
        }

        Ok(())
    }

    /// Returns a string uniquely representing the connection.
    ///
    /// This can be used for logging purposes to differentiate between multiple
    /// connections.
    #[inline]
    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    /// Returns the negotiated ALPN protocol.
    ///
    /// If no protocol has been negotiated, the returned value is empty.
    #[inline]
    pub fn application_proto(&self) -> &[u8] {
        self.alpn.as_ref()
    }

    /// Returns the server name requested by the client.
    #[inline]
    pub fn server_name(&self) -> Option<&str> {
        self.handshake.server_name()
    }

    /// Returns the peer's leaf certificate (if any) as a DER-encoded buffer.
    #[inline]
    pub fn peer_cert(&self) -> Option<&[u8]> {
        self.handshake.peer_cert()
    }

    /// Returns the peer's certificate chain (if any) as a vector of DER-encoded
    /// buffers.
    ///
    /// The certificate at index 0 is the peer's leaf certificate, the other
    /// certificates (if any) are the chain certificate authorities used to
    /// sign the leaf certificate.
    #[inline]
    pub fn peer_cert_chain(&self) -> Option<Vec<&[u8]>> {
        self.handshake.peer_cert_chain()
    }

    /// Returns the serialized cryptographic session for the connection.
    ///
    /// This can be used by a client to cache a connection's session, and resume
    /// it later using the [`set_session()`] method.
    ///
    /// [`set_session()`]: struct.Connection.html#method.set_session
    #[inline]
    pub fn session(&self) -> Option<&[u8]> {
        self.session.as_deref()
    }

    /// Returns the source connection ID.
    ///
    /// When there are multiple IDs, and if there is an active path, the ID used
    /// on that path is returned. Otherwise the oldest ID is returned.
    ///
    /// Note that the value returned can change throughout the connection's
    /// lifetime.
    #[inline]
    pub fn source_id(&self) -> ConnectionId {
        if let Ok(path) = self.paths.get_active() {
            if let Some(active_scid_seq) = path.active_scid_seq {
                if let Ok(e) = self.ids.get_scid(active_scid_seq) {
                    return ConnectionId::from_ref(e.cid.as_ref());
                }
            }
        }

        let e = self.ids.oldest_scid();
        ConnectionId::from_ref(e.cid.as_ref())
    }

    /// Returns all active source connection IDs.
    ///
    /// An iterator is returned for all active IDs (i.e. ones that have not
    /// been explicitly retired yet).
    #[inline]
    pub fn source_ids(&self) -> impl Iterator<Item = &ConnectionId> {
        self.ids.scids_iter()
    }

    /// Returns the destination connection ID.
    ///
    /// Note that the value returned can change throughout the connection's
    /// lifetime.
    #[inline]
    pub fn destination_id(&self) -> ConnectionId {
        if let Ok(path) = self.paths.get_active() {
            if let Some(active_dcid_seq) = path.active_dcid_seq {
                if let Ok(e) = self.ids.get_dcid(active_dcid_seq) {
                    return ConnectionId::from_ref(e.cid.as_ref());
                }
            }
        }

        let e = self.ids.oldest_dcid();
        ConnectionId::from_ref(e.cid.as_ref())
    }

    /// Returns true if the connection handshake is complete.
    #[inline]
    pub fn is_established(&self) -> bool {
        self.handshake_completed
    }

    /// Returns true if the connection is resumed.
    #[inline]
    pub fn is_resumed(&self) -> bool {
        self.handshake.is_resumed()
    }

    /// Returns true if the connection has a pending handshake that has
    /// progressed enough to send or receive early data.
    #[inline]
    pub fn is_in_early_data(&self) -> bool {
        self.handshake.is_in_early_data()
    }

    /// Returns whether there is stream or DATAGRAM data available to read.
    #[inline]
    pub fn is_readable(&self) -> bool {
        self.streams.has_readable() || self.dgram_recv_front_len().is_some()
    }

    /// Returns whether the network path with local address `from` and remote
    /// address `peer` has been validated.
    ///
    /// If the 4-tuple does not exist over the connection, returns an
    /// [`InvalidState`].
    ///
    /// [`InvalidState`]: enum.Error.html#variant.InvalidState
    pub fn is_path_validated(
        &self, from: SocketAddr, to: SocketAddr,
    ) -> Result<bool> {
        let pid = self
            .paths
            .path_id_from_addrs(&(from, to))
            .ok_or(Error::InvalidState)?;

        Ok(self.paths.get(pid)?.validated())
    }

    /// Returns true if the connection is draining.
    ///
    /// If this returns `true`, the connection object cannot yet be dropped, but
    /// no new application data can be sent or received. An application should
    /// continue calling the [`recv()`], [`timeout()`], and [`on_timeout()`]
    /// methods as normal, until the [`is_closed()`] method returns `true`.
    ///
    /// In contrast, once `is_draining()` returns `true`, calling [`send()`]
    /// is not required because no new outgoing packets will be generated.
    ///
    /// [`recv()`]: struct.Connection.html#method.recv
    /// [`send()`]: struct.Connection.html#method.send
    /// [`timeout()`]: struct.Connection.html#method.timeout
    /// [`on_timeout()`]: struct.Connection.html#method.on_timeout
    /// [`is_closed()`]: struct.Connection.html#method.is_closed
    #[inline]
    pub fn is_draining(&self) -> bool {
        self.draining_timer.is_some()
    }

    /// Returns true if the connection is closed.
    ///
    /// If this returns true, the connection object can be dropped.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Returns true if the connection was closed due to the idle timeout.
    #[inline]
    pub fn is_timed_out(&self) -> bool {
        self.timed_out
    }

    /// Returns the error received from the peer, if any.
    ///
    /// Note that a `Some` return value does not necessarily imply
    /// [`is_closed()`] or any other connection state.
    ///
    /// [`is_closed()`]: struct.Connection.html#method.is_closed
    #[inline]
    pub fn peer_error(&self) -> Option<&ConnectionError> {
        self.peer_error.as_ref()
    }

    /// Returns the error [`close()`] was called with, or internally
    /// created quiche errors, if any.
    ///
    /// Note that a `Some` return value does not necessarily imply
    /// [`is_closed()`] or any other connection state.
    /// `Some` also does not guarantee that the error has been sent to
    /// or received by the peer.
    ///
    /// [`close()`]: struct.Connection.html#method.close
    /// [`is_closed()`]: struct.Connection.html#method.is_closed
    #[inline]
    pub fn local_error(&self) -> Option<&ConnectionError> {
        self.local_error.as_ref()
    }

    /// Collects and returns statistics about the connection.
    #[inline]
    pub fn stats(&self) -> Stats {
        Stats {
            recv: self.recv_count,
            sent: self.sent_count,
            lost: self.lost_count,
            retrans: self.retrans_count,
            sent_bytes: self.sent_bytes,
            recv_bytes: self.recv_bytes,
            acked_bytes: self.acked_bytes,
            lost_bytes: self.lost_bytes,
            stream_retrans_bytes: self.stream_retrans_bytes,
            paths_count: self.paths.len(),
            reset_stream_count_local: self.reset_stream_local_count,
            stopped_stream_count_local: self.stopped_stream_local_count,
            reset_stream_count_remote: self.reset_stream_remote_count,
            stopped_stream_count_remote: self.stopped_stream_remote_count,
            path_challenge_rx_count: self.path_challenge_rx_count,
        }
    }

    /// Returns reference to peer's transport parameters. Returns `None` if we
    /// have not yet processed the peer's transport parameters.
    pub fn peer_transport_params(&self) -> Option<&TransportParams> {
        if !self.parsed_peer_transport_params {
            return None;
        }

        Some(&self.peer_transport_params)
    }

    /// Collects and returns statistics about each known path for the
    /// connection.
    pub fn path_stats(&self) -> impl Iterator<Item = PathStats> + '_ {
        self.paths.iter().map(|(_, p)| p.stats())
    }

    /// Returns whether or not this is a server-side connection.
    pub fn is_server(&self) -> bool {
        self.is_server
    }

    fn encode_transport_params(&mut self) -> Result<()> {
        let mut raw_params = [0; 128];

        let raw_params = TransportParams::encode(
            &self.local_transport_params,
            self.is_server,
            &mut raw_params,
        )?;

        self.handshake.set_quic_transport_params(raw_params)?;

        Ok(())
    }

    fn parse_peer_transport_params(
        &mut self, peer_params: TransportParams,
    ) -> Result<()> {
        // Validate initial_source_connection_id.
        match &peer_params.initial_source_connection_id {
            Some(v) if v != &self.destination_id() =>
                return Err(Error::InvalidTransportParam),

            Some(_) => (),

            // initial_source_connection_id must be sent by
            // both endpoints.
            None => return Err(Error::InvalidTransportParam),
        }

        // Validate original_destination_connection_id.
        if let Some(odcid) = &self.odcid {
            match &peer_params.original_destination_connection_id {
                Some(v) if v != odcid =>
                    return Err(Error::InvalidTransportParam),

                Some(_) => (),

                // original_destination_connection_id must be
                // sent by the server.
                None if !self.is_server =>
                    return Err(Error::InvalidTransportParam),

                None => (),
            }
        }

        // Validate retry_source_connection_id.
        if let Some(rscid) = &self.rscid {
            match &peer_params.retry_source_connection_id {
                Some(v) if v != rscid =>
                    return Err(Error::InvalidTransportParam),

                Some(_) => (),

                // retry_source_connection_id must be sent by
                // the server.
                None => return Err(Error::InvalidTransportParam),
            }
        }

        self.process_peer_transport_params(peer_params)?;

        self.parsed_peer_transport_params = true;

        Ok(())
    }

    fn process_peer_transport_params(
        &mut self, peer_params: TransportParams,
    ) -> Result<()> {
        self.max_tx_data = peer_params.initial_max_data;

        // Update send capacity.
        self.update_tx_cap();

        self.streams
            .update_peer_max_streams_bidi(peer_params.initial_max_streams_bidi);
        self.streams
            .update_peer_max_streams_uni(peer_params.initial_max_streams_uni);

        let max_ack_delay =
            time::Duration::from_millis(peer_params.max_ack_delay);

        self.recovery_config.max_ack_delay = max_ack_delay;

        let active_path = self.paths.get_active_mut()?;

        active_path.recovery.update_max_ack_delay(max_ack_delay);

        if active_path.pmtud.get_probe_status() {
            active_path.recovery.pmtud_update_max_datagram_size(
                active_path
                    .pmtud
                    .get_probe_size()
                    .min(peer_params.max_udp_payload_size as usize),
            );
        } else {
            active_path.recovery.update_max_datagram_size(
                peer_params.max_udp_payload_size as usize,
            );
        }

        // Record the max_active_conn_id parameter advertised by the peer.
        self.ids
            .set_source_conn_id_limit(peer_params.active_conn_id_limit);

        self.peer_transport_params = peer_params;

        Ok(())
    }

    /// Continues the handshake.
    ///
    /// If the connection is already established, it does nothing.
    fn do_handshake(&mut self, now: time::Instant) -> Result<()> {
        let mut ex_data = tls::ExData {
            application_protos: &self.application_protos,

            pkt_num_spaces: &mut self.pkt_num_spaces,

            session: &mut self.session,

            local_error: &mut self.local_error,

            keylog: self.keylog.as_mut(),

            trace_id: &self.trace_id,

            is_server: self.is_server,
        };

        if self.handshake_completed {
            return self.handshake.process_post_handshake(&mut ex_data);
        }

        match self.handshake.do_handshake(&mut ex_data) {
            Ok(_) => (),

            Err(Error::Done) => {
                info!("Do handshake");
                // Try to parse transport parameters as soon as the first flight
                // of handshake data is processed.
                //
                // This is potentially dangerous as the handshake hasn't been
                // completed yet, though it's required to be able to send data
                // in 0.5 RTT.
                let raw_params = self.handshake.quic_transport_params();

                if !self.parsed_peer_transport_params && !raw_params.is_empty() {
                    let peer_params =
                        TransportParams::decode(raw_params, self.is_server)?;

                    info!("Parse peer transport params: {:?}", peer_params);
                    self.parse_peer_transport_params(peer_params)?;
                }

                info!("Lies here in do handshake...");

                return Ok(());
            },

            Err(e) => return Err(e),
        };

        self.handshake_completed = self.handshake.is_completed();

        self.alpn = self.handshake.alpn_protocol().to_vec();

        let raw_params = self.handshake.quic_transport_params();

        if !self.parsed_peer_transport_params && !raw_params.is_empty() {
            let peer_params =
                TransportParams::decode(raw_params, self.is_server)?;

            self.parse_peer_transport_params(peer_params)?;
        }

        info!("HANDSHAKE COMPLETED: {}", self.handshake_completed);

        if self.handshake_completed {
            // The handshake is considered confirmed at the server when the
            // handshake completes, at which point we can also drop the
            // handshake epoch.
            if self.is_server {
                self.handshake_confirmed = true;

                self.drop_epoch_state(packet::Epoch::Handshake, now);
            }

            // Once the handshake is completed there's no point in processing
            // 0-RTT packets anymore, so clear the buffer now.
            self.undecryptable_pkts.clear();

            trace!("{} connection established: proto={:?} cipher={:?} curve={:?} sigalg={:?} resumed={} {:?}",
                   &self.trace_id,
                   std::str::from_utf8(self.application_proto()),
                   self.handshake.cipher(),
                   self.handshake.curve(),
                   self.handshake.sigalg(),
                   self.handshake.is_resumed(),
                   self.peer_transport_params);
        }

        Ok(())
    }

    /// Selects the packet type for the next outgoing packet.
    fn write_pkt_type(&self, send_pid: usize) -> Result<packet::Type> {
        // On error send packet in the latest epoch available, but only send
        // 1-RTT ones when the handshake is completed.
        if self
            .local_error
            .as_ref()
            .map_or(false, |conn_err| !conn_err.is_app)
        {
            let epoch = match self.handshake.write_level() {
                crypto::Level::Initial => packet::Epoch::Initial,
                crypto::Level::ZeroRTT => unreachable!(),
                crypto::Level::Handshake => packet::Epoch::Handshake,
                crypto::Level::OneRTT => packet::Epoch::Application,
            };

            if !self.handshake_confirmed {
                match epoch {
                    // Downgrade the epoch to Handshake as the handshake is not
                    // completed yet.
                    packet::Epoch::Application =>
                        return Ok(packet::Type::Handshake),

                    // Downgrade the epoch to Initial as the remote peer might
                    // not be able to decrypt handshake packets yet.
                    packet::Epoch::Handshake
                        if self.pkt_num_spaces[packet::Epoch::Initial]
                            .has_keys() =>
                        return Ok(packet::Type::Initial),

                    _ => (),
                };
            }

            return Ok(packet::Type::from_epoch(epoch));
        }

        for &epoch in packet::Epoch::epochs(
            packet::Epoch::Initial..=packet::Epoch::Application,
        ) {
            // Only send packets in a space when we have the send keys for it.
            if self.pkt_num_spaces[epoch].crypto_seal.is_none() {
                continue;
            }

            // We are ready to send data for this packet number space.
            if self.pkt_num_spaces[epoch].ready() {
                return Ok(packet::Type::from_epoch(epoch));
            }

            // There are lost frames in this packet number space.
            for (_, p) in self.paths.iter() {
                if p.recovery.has_lost_frames(epoch) {
                    return Ok(packet::Type::from_epoch(epoch));
                }

                // We need to send PTO probe packets.
                if p.recovery.loss_probes(epoch) > 0 {
                    return Ok(packet::Type::from_epoch(epoch));
                }
            }
        }

        // If there are flushable, almost full or blocked streams, use the
        // Application epoch.
        let send_path = self.paths.get(send_pid)?;
        if (self.is_established() || self.is_in_early_data()) &&
            (self.should_send_handshake_done() ||
                self.almost_full ||
                self.blocked_limit.is_some() ||
                self.dgram_send_queue.has_pending() ||
                self.local_error
                    .as_ref()
                    .map_or(false, |conn_err| conn_err.is_app) ||
                self.streams.should_update_max_streams_bidi() ||
                self.streams.should_update_max_streams_uni() ||
                self.streams.has_flushable() ||
                self.streams.has_almost_full() ||
                self.streams.has_blocked() ||
                self.streams.has_reset() ||
                self.streams.has_stopped() ||
                self.ids.has_new_scids() ||
                self.ids.has_retire_dcids() ||
                send_path.pmtud.get_probe_status() ||
                send_path.needs_ack_eliciting ||
                send_path.probing_required())
        {
            // Only clients can send 0-RTT packets.
            if !self.is_server && self.is_in_early_data() {
                return Ok(packet::Type::ZeroRTT);
            }

            return Ok(packet::Type::Short);
        }

        Err(Error::Done)
    }

    /// Returns the mutable stream with the given ID if it exists, or creates
    /// a new one otherwise.
    fn get_or_create_stream(
        &mut self, id: u64, local: bool,
    ) -> Result<&mut stream::Stream> {
        self.streams.get_or_create(
            id,
            &self.local_transport_params,
            &self.peer_transport_params,
            local,
            self.is_server,
        )
    }

    /// Processes an incoming frame.
    fn process_frame(
        &mut self, frame: frame::Frame, hdr: &packet::Header,
        recv_path_id: usize, epoch: packet::Epoch, now: time::Instant,
    ) -> Result<()> {
        trace!("{} rx frm {:?}", self.trace_id, frame);

        match frame {
            frame::Frame::Padding { .. } => (),

            frame::Frame::Ping { .. } => (),

            frame::Frame::ACK {
                ranges, ack_delay, ..
            } => {
                let ack_delay = ack_delay
                    .checked_mul(2_u64.pow(
                        self.peer_transport_params.ack_delay_exponent as u32,
                    ))
                    .ok_or(Error::InvalidFrame)?;

                if epoch == packet::Epoch::Handshake ||
                    (epoch == packet::Epoch::Application &&
                        self.is_established())
                {
                    self.peer_verified_initial_address = true;
                }

                let handshake_status = self.handshake_status();

                let is_app_limited = self.delivery_rate_check_if_app_limited();

                for (_, p) in self.paths.iter_mut() {
                    if is_app_limited {
                        p.recovery.delivery_rate_update_app_limited(true);
                    }

                    let (lost_packets, lost_bytes, acked_bytes) =
                        p.recovery.on_ack_received(
                            &ranges,
                            ack_delay,
                            epoch,
                            handshake_status,
                            now,
                            &self.trace_id,
                        )?;

                    self.lost_count += lost_packets;
                    self.lost_bytes += lost_bytes as u64;
                    self.acked_bytes += acked_bytes as u64;
                }
            },

            frame::Frame::ResetStream {
                stream_id,
                error_code,
                final_size,
            } => {
                // Peer can't send on our unidirectional streams.
                if !stream::is_bidi(stream_id) &&
                    stream::is_local(stream_id, self.is_server)
                {
                    return Err(Error::InvalidStreamState(stream_id));
                }

                let max_rx_data_left = self.max_rx_data() - self.rx_data;

                // Get existing stream or create a new one, but if the stream
                // has already been closed and collected, ignore the frame.
                //
                // This can happen if e.g. an ACK frame is lost, and the peer
                // retransmits another frame before it realizes that the stream
                // is gone.
                //
                // Note that it makes it impossible to check if the frame is
                // illegal, since we have no state, but since we ignore the
                // frame, it should be fine.
                let stream = match self.get_or_create_stream(stream_id, false) {
                    Ok(v) => v,

                    Err(Error::Done) => return Ok(()),

                    Err(e) => return Err(e),
                };

                let was_readable = stream.is_readable();
                let priority_key = Arc::clone(&stream.priority_key);

                let max_off_delta =
                    stream.recv.reset(error_code, final_size)? as u64;

                if max_off_delta > max_rx_data_left {
                    return Err(Error::FlowControl);
                }

                if !was_readable && stream.is_readable() {
                    self.streams.insert_readable(&priority_key);
                }

                self.rx_data += max_off_delta;

                self.reset_stream_remote_count =
                    self.reset_stream_remote_count.saturating_add(1);
            },

            frame::Frame::StopSending {
                stream_id,
                error_code,
            } => {
                // STOP_SENDING on a receive-only stream is a fatal error.
                if !stream::is_local(stream_id, self.is_server) &&
                    !stream::is_bidi(stream_id)
                {
                    return Err(Error::InvalidStreamState(stream_id));
                }

                // Get existing stream or create a new one, but if the stream
                // has already been closed and collected, ignore the frame.
                //
                // This can happen if e.g. an ACK frame is lost, and the peer
                // retransmits another frame before it realizes that the stream
                // is gone.
                //
                // Note that it makes it impossible to check if the frame is
                // illegal, since we have no state, but since we ignore the
                // frame, it should be fine.
                let stream = match self.get_or_create_stream(stream_id, false) {
                    Ok(v) => v,

                    Err(Error::Done) => return Ok(()),

                    Err(e) => return Err(e),
                };

                let was_writable = stream.is_writable();

                let priority_key = Arc::clone(&stream.priority_key);

                // Try stopping the stream.
                if let Ok((final_size, unsent)) = stream.send.stop(error_code) {
                    // Claw back some flow control allowance from data that was
                    // buffered but not actually sent before the stream was
                    // reset.
                    //
                    // Note that `tx_cap` will be updated later on, so no need
                    // to touch it here.
                    self.tx_data = self.tx_data.saturating_sub(unsent);

                    self.tx_buffered =
                        self.tx_buffered.saturating_sub(unsent as usize);

                    self.streams.insert_reset(stream_id, error_code, final_size);

                    if !was_writable {
                        self.streams.insert_writable(&priority_key);
                    }

                    self.stopped_stream_remote_count =
                        self.stopped_stream_remote_count.saturating_add(1);
                    self.reset_stream_local_count =
                        self.reset_stream_local_count.saturating_add(1);
                }
            },

            frame::Frame::Crypto { data } => {
                info!("RECEIVE FRAME CRYPTO");
                if data.max_off() >= MAX_CRYPTO_STREAM_OFFSET {
                    return Err(Error::CryptoBufferExceeded);
                }

                // Push the data to the stream so it can be re-ordered.
                self.pkt_num_spaces[epoch].crypto_stream.recv.write(data)?;

                // Feed crypto data to the TLS state, if there's data
                // available at the expected offset.
                let mut crypto_buf = [0; 512];

                let level = crypto::Level::from_epoch(epoch);

                let stream = &mut self.pkt_num_spaces[epoch].crypto_stream;

                let mut total = 0;
                while let Ok((read, _)) = stream.recv.emit(&mut crypto_buf) {
                    total += read;
                    let recv_buf = &crypto_buf[..read];
                    self.handshake.provide_data(level, recv_buf)?;
                }

                info!("FED {total} bytes to the crypto at epoch {epoch:?}");

                self.do_handshake(now)?;
            },

            frame::Frame::CryptoHeader { .. } => unreachable!(),

            // TODO: implement stateless retry
            frame::Frame::NewToken { .. } =>
                if self.is_server {
                    return Err(Error::InvalidPacket);
                },

            frame::Frame::Stream { stream_id, data } => {
                // Peer can't send on our unidirectional streams.
                if !stream::is_bidi(stream_id) &&
                    stream::is_local(stream_id, self.is_server)
                {
                    return Err(Error::InvalidStreamState(stream_id));
                }

                let max_rx_data_left = self.max_rx_data() - self.rx_data;

                // Get existing stream or create a new one, but if the stream
                // has already been closed and collected, ignore the frame.
                //
                // This can happen if e.g. an ACK frame is lost, and the peer
                // retransmits another frame before it realizes that the stream
                // is gone.
                //
                // Note that it makes it impossible to check if the frame is
                // illegal, since we have no state, but since we ignore the
                // frame, it should be fine.
                let stream = match self.get_or_create_stream(stream_id, false) {
                    Ok(v) => v,

                    Err(Error::Done) => return Ok(()),

                    Err(e) => return Err(e),
                };

                // Check for the connection-level flow control limit.
                let max_off_delta =
                    data.max_off().saturating_sub(stream.recv.max_off());

                if max_off_delta > max_rx_data_left {
                    return Err(Error::FlowControl);
                }

                let was_readable = stream.is_readable();
                let priority_key = Arc::clone(&stream.priority_key);

                let was_draining = stream.recv.is_draining();

                stream.recv.write(data)?;

                if !was_readable && stream.is_readable() {
                    self.streams.insert_readable(&priority_key);
                }

                self.rx_data += max_off_delta;

                if was_draining {
                    // When a stream is in draining state it will not queue
                    // incoming data for the application to read, so consider
                    // the received data as consumed, which might trigger a flow
                    // control update.
                    self.flow_control.add_consumed(max_off_delta);

                    if self.should_update_max_data() {
                        self.almost_full = true;
                    }
                }
            },

            frame::Frame::StreamHeader { .. } => unreachable!(),

            frame::Frame::MaxData { max } => {
                self.max_tx_data = cmp::max(self.max_tx_data, max);
            },

            frame::Frame::MaxStreamData { stream_id, max } => {
                // Peer can't receive on its own unidirectional streams.
                if !stream::is_bidi(stream_id) &&
                    !stream::is_local(stream_id, self.is_server)
                {
                    return Err(Error::InvalidStreamState(stream_id));
                }

                // Get existing stream or create a new one, but if the stream
                // has already been closed and collected, ignore the frame.
                //
                // This can happen if e.g. an ACK frame is lost, and the peer
                // retransmits another frame before it realizes that the stream
                // is gone.
                //
                // Note that it makes it impossible to check if the frame is
                // illegal, since we have no state, but since we ignore the
                // frame, it should be fine.
                let stream = match self.get_or_create_stream(stream_id, false) {
                    Ok(v) => v,

                    Err(Error::Done) => return Ok(()),

                    Err(e) => return Err(e),
                };

                let was_flushable = stream.is_flushable();

                stream.send.update_max_data(max);

                let writable = stream.is_writable();

                let priority_key = Arc::clone(&stream.priority_key);

                // If the stream is now flushable push it to the flushable queue,
                // but only if it wasn't already queued.
                if stream.is_flushable() && !was_flushable {
                    let priority_key = Arc::clone(&stream.priority_key);
                    self.streams.insert_flushable(&priority_key);
                }

                if writable {
                    self.streams.insert_writable(&priority_key);
                }
            },

            frame::Frame::MaxStreamsBidi { max } => {
                if max > MAX_STREAM_ID {
                    return Err(Error::InvalidFrame);
                }

                self.streams.update_peer_max_streams_bidi(max);
            },

            frame::Frame::MaxStreamsUni { max } => {
                if max > MAX_STREAM_ID {
                    return Err(Error::InvalidFrame);
                }

                self.streams.update_peer_max_streams_uni(max);
            },

            frame::Frame::DataBlocked { .. } => (),

            frame::Frame::StreamDataBlocked { .. } => (),

            frame::Frame::StreamsBlockedBidi { limit } => {
                if limit > MAX_STREAM_ID {
                    return Err(Error::InvalidFrame);
                }
            },

            frame::Frame::StreamsBlockedUni { limit } => {
                if limit > MAX_STREAM_ID {
                    return Err(Error::InvalidFrame);
                }
            },

            frame::Frame::NewConnectionId {
                seq_num,
                retire_prior_to,
                conn_id,
                reset_token,
            } => {
                if self.ids.zero_length_dcid() {
                    return Err(Error::InvalidState);
                }

                let mut retired_path_ids = SmallVec::new();

                // Retire pending path IDs before propagating the error code to
                // make sure retired connection IDs are not in use anymore.
                let new_dcid_res = self.ids.new_dcid(
                    conn_id.into(),
                    seq_num,
                    u128::from_be_bytes(reset_token),
                    retire_prior_to,
                    &mut retired_path_ids,
                );

                for (dcid_seq, pid) in retired_path_ids {
                    let path = self.paths.get_mut(pid)?;

                    // Maybe the path already switched to another DCID.
                    if path.active_dcid_seq != Some(dcid_seq) {
                        continue;
                    }

                    if let Some(new_dcid_seq) =
                        self.ids.lowest_available_dcid_seq()
                    {
                        path.active_dcid_seq = Some(new_dcid_seq);

                        self.ids.link_dcid_to_path_id(new_dcid_seq, pid)?;

                        trace!(
                            "{} path ID {} changed DCID: old seq num {} new seq num {}",
                            self.trace_id, pid, dcid_seq, new_dcid_seq,
                        );
                    } else {
                        // We cannot use this path anymore for now.
                        path.active_dcid_seq = None;

                        trace!(
                            "{} path ID {} cannot be used; DCID seq num {} has been retired",
                            self.trace_id, pid, dcid_seq,
                        );
                    }
                }

                // Propagate error (if any) now...
                new_dcid_res?;
            },

            frame::Frame::RetireConnectionId { seq_num } => {
                if self.ids.zero_length_scid() {
                    return Err(Error::InvalidState);
                }

                if let Some(pid) = self.ids.retire_scid(seq_num, &hdr.dcid)? {
                    let path = self.paths.get_mut(pid)?;

                    // Maybe we already linked a new SCID to that path.
                    if path.active_scid_seq == Some(seq_num) {
                        // XXX: We do not remove unused paths now, we instead
                        // wait until we need to maintain more paths than the
                        // host is willing to.
                        path.active_scid_seq = None;
                    }
                }
            },

            frame::Frame::PathChallenge { data } => {
                self.path_challenge_rx_count += 1;

                self.paths
                    .get_mut(recv_path_id)?
                    .on_challenge_received(data);
            },

            frame::Frame::PathResponse { data } => {
                self.paths.on_response_received(data)?;
            },

            frame::Frame::ConnectionClose {
                error_code, reason, ..
            } => {
                self.peer_error = Some(ConnectionError {
                    is_app: false,
                    error_code,
                    reason,
                });

                let path = self.paths.get_active()?;
                self.draining_timer = Some(now + (path.recovery.pto() * 3));
            },

            frame::Frame::ApplicationClose { error_code, reason } => {
                self.peer_error = Some(ConnectionError {
                    is_app: true,
                    error_code,
                    reason,
                });

                let path = self.paths.get_active()?;
                self.draining_timer = Some(now + (path.recovery.pto() * 3));
            },

            frame::Frame::HandshakeDone => {
                info!("CLIENT RECEIVES FRAME HANDSHAKE DONE");
                if self.is_server {
                    return Err(Error::InvalidPacket);
                }

                self.peer_verified_initial_address = true;

                self.handshake_confirmed = true;

                // Once the handshake is confirmed, we can drop Handshake keys.
                self.drop_epoch_state(packet::Epoch::Handshake, now);
            },

            frame::Frame::Datagram { data } => {
                // Close the connection if DATAGRAMs are not enabled.
                // quiche always advertises support for 64K sized DATAGRAM
                // frames, as recommended by the standard, so we don't need a
                // size check.
                if !self.dgram_enabled() {
                    return Err(Error::InvalidState);
                }

                // If recv queue is full, discard oldest
                if self.dgram_recv_queue.is_full() {
                    self.dgram_recv_queue.pop();
                }

                self.dgram_recv_queue.push(data)?;
            },

            frame::Frame::DatagramHeader { .. } => unreachable!(),
        }

        Ok(())
    }

    /// Drops the keys and recovery state for the given epoch.
    fn drop_epoch_state(&mut self, epoch: packet::Epoch, now: time::Instant) {
        if self.pkt_num_spaces[epoch].crypto_open.is_none() {
            return;
        }

        self.pkt_num_spaces[epoch].crypto_open = None;
        self.pkt_num_spaces[epoch].crypto_seal = None;
        self.pkt_num_spaces[epoch].clear();

        let handshake_status = self.handshake_status();
        for (_, p) in self.paths.iter_mut() {
            p.recovery
                .on_pkt_num_space_discarded(epoch, handshake_status, now);
        }

        trace!("{} dropped epoch {} state", self.trace_id, epoch);
    }

    /// Returns true if the connection-level flow control needs to be updated.
    ///
    /// This happens when the new max data limit is at least double the amount
    /// of data that can be received before blocking.
    fn should_update_max_data(&self) -> bool {
        self.flow_control.should_update_max_data()
    }

    /// Returns the connection level flow control limit.
    fn max_rx_data(&self) -> u64 {
        self.flow_control.max_data()
    }

    /// Returns true if the HANDSHAKE_DONE frame needs to be sent.
    fn should_send_handshake_done(&self) -> bool {
        self.is_established() && !self.handshake_done_sent && self.is_server
    }

    /// Returns the idle timeout value.
    ///
    /// `None` is returned if both end-points disabled the idle timeout.
    fn idle_timeout(&mut self) -> Option<time::Duration> {
        // If the transport parameter is set to 0, then the respective endpoint
        // decided to disable the idle timeout. If both are disabled we should
        // not set any timeout.
        if self.local_transport_params.max_idle_timeout == 0 &&
            self.peer_transport_params.max_idle_timeout == 0
        {
            return None;
        }

        // If the local endpoint or the peer disabled the idle timeout, use the
        // other peer's value, otherwise use the minimum of the two values.
        let idle_timeout = if self.local_transport_params.max_idle_timeout == 0 {
            self.peer_transport_params.max_idle_timeout
        } else if self.peer_transport_params.max_idle_timeout == 0 {
            self.local_transport_params.max_idle_timeout
        } else {
            cmp::min(
                self.local_transport_params.max_idle_timeout,
                self.peer_transport_params.max_idle_timeout,
            )
        };

        let path_pto = match self.paths.get_active() {
            Ok(p) => p.recovery.pto(),
            Err(_) => time::Duration::ZERO,
        };

        let idle_timeout = time::Duration::from_millis(idle_timeout);
        let idle_timeout = cmp::max(idle_timeout, 3 * path_pto);

        Some(idle_timeout)
    }

    /// Returns the connection's handshake status for use in loss recovery.
    fn handshake_status(&self) -> recovery::HandshakeStatus {
        recovery::HandshakeStatus {
            has_handshake_keys: self.pkt_num_spaces[packet::Epoch::Handshake]
                .has_keys(),

            peer_verified_address: self.peer_verified_initial_address,

            completed: self.is_established(),
        }
    }

    /// Updates send capacity.
    fn update_tx_cap(&mut self) {
        let cwin_available = match self.paths.get_active() {
            Ok(p) => p.recovery.cwnd_available() as u64,
            Err(_) => 0,
        };

        self.tx_cap =
            cmp::min(cwin_available, self.max_tx_data - self.tx_data) as usize;
    }

    fn delivery_rate_check_if_app_limited(&self) -> bool {
        // Enter the app-limited phase of delivery rate when these conditions
        // are met:
        //
        // - The remaining capacity is higher than available bytes in cwnd (there
        //   is more room to send).
        // - New data since the last send() is smaller than available bytes in
        //   cwnd (we queued less than what we can send).
        // - There is room to send more data in cwnd.
        //
        // In application-limited phases the transmission rate is limited by the
        // application rather than the congestion control algorithm.
        //
        // Note that this is equivalent to CheckIfApplicationLimited() from the
        // delivery rate draft. This is also separate from `recovery.app_limited`
        // and only applies to delivery rate calculation.
        let cwin_available = self
            .paths
            .iter()
            .filter(|&(_, p)| p.active())
            .map(|(_, p)| p.recovery.cwnd_available())
            .sum();

        ((self.tx_buffered + self.dgram_send_queue_byte_size()) < cwin_available) &&
            (self.tx_data.saturating_sub(self.last_tx_data)) <
                cwin_available as u64 &&
            cwin_available > 0
    }

    fn set_initial_dcid(
        &mut self, cid: ConnectionId<'static>, reset_token: Option<u128>,
        path_id: usize,
    ) -> Result<()> {
        self.ids.set_initial_dcid(cid, reset_token, Some(path_id));
        self.paths.get_mut(path_id)?.active_dcid_seq = Some(0);

        Ok(())
    }

    /// Selects the path that the incoming packet belongs to, or creates a new
    /// one if no existing path matches.
    fn get_or_create_recv_path_id(
        &mut self, recv_pid: Option<usize>, dcid: &ConnectionId, buf_len: usize,
        info: &RecvInfo,
    ) -> Result<usize> {
        let ids = &mut self.ids;

        let (in_scid_seq, mut in_scid_pid) =
            ids.find_scid_seq(dcid).ok_or(Error::InvalidState)?;

        if let Some(recv_pid) = recv_pid {
            // If the path observes a change of SCID used, note it.
            let recv_path = self.paths.get_mut(recv_pid)?;

            let cid_entry =
                recv_path.active_scid_seq.and_then(|v| ids.get_scid(v).ok());

            if cid_entry.map(|e| &e.cid) != Some(dcid) {
                let incoming_cid_entry = ids.get_scid(in_scid_seq)?;

                let prev_recv_pid =
                    incoming_cid_entry.path_id.unwrap_or(recv_pid);

                if prev_recv_pid != recv_pid {
                    trace!(
                        "{} peer reused CID {:?} from path {} on path {}",
                        self.trace_id,
                        dcid,
                        prev_recv_pid,
                        recv_pid
                    );

                    // TODO: reset congestion control.
                }

                trace!(
                    "{} path ID {} now see SCID with seq num {}",
                    self.trace_id,
                    recv_pid,
                    in_scid_seq
                );

                recv_path.active_scid_seq = Some(in_scid_seq);
                ids.link_scid_to_path_id(in_scid_seq, recv_pid)?;
            }

            return Ok(recv_pid);
        }

        // This is a new 4-tuple. See if the CID has not been assigned on
        // another path.

        // Ignore this step if are using zero-length SCID.
        if ids.zero_length_scid() {
            in_scid_pid = None;
        }

        if let Some(in_scid_pid) = in_scid_pid {
            // This CID has been used by another path. If we have the
            // room to do so, create a new `Path` structure holding this
            // new 4-tuple. Otherwise, drop the packet.
            let old_path = self.paths.get_mut(in_scid_pid)?;
            let old_local_addr = old_path.local_addr();
            let old_peer_addr = old_path.peer_addr();

            trace!(
                "{} reused CID seq {} of ({},{}) (path {}) on ({},{})",
                self.trace_id,
                in_scid_seq,
                old_local_addr,
                old_peer_addr,
                in_scid_pid,
                info.to,
                info.from
            );

            // Notify the application.
            self.paths
                .notify_event(path::PathEvent::ReusedSourceConnectionId(
                    in_scid_seq,
                    (old_local_addr, old_peer_addr),
                    (info.to, info.from),
                ));
        }

        // This is a new path using an unassigned CID; create it!
        let mut path = path::Path::new(
            info.to,
            info.from,
            &self.recovery_config,
            self.path_challenge_recv_max_queue_len,
            MIN_CLIENT_INITIAL_LEN,
            false,
        );

        path.max_send_bytes = buf_len * self.max_amplification_factor;
        path.active_scid_seq = Some(in_scid_seq);

        // Automatically probes the new path.
        path.request_validation();

        let pid = self.paths.insert_path(path, self.is_server)?;

        // Do not record path reuse.
        if in_scid_pid.is_none() {
            ids.link_scid_to_path_id(in_scid_seq, pid)?;
        }

        Ok(pid)
    }

    /// Selects the path on which the next packet must be sent.
    fn get_send_path_id(
        &self, from: Option<SocketAddr>, to: Option<SocketAddr>,
    ) -> Result<usize> {
        // A probing packet must be sent, but only if the connection is fully
        // established.
        if self.is_established() {
            let mut probing = self
                .paths
                .iter()
                .filter(|(_, p)| from.is_none() || Some(p.local_addr()) == from)
                .filter(|(_, p)| to.is_none() || Some(p.peer_addr()) == to)
                .filter(|(_, p)| p.active_dcid_seq.is_some())
                .filter(|(_, p)| p.probing_required())
                .map(|(pid, _)| pid);

            if let Some(pid) = probing.next() {
                return Ok(pid);
            }
        }

        if let Some((pid, p)) = self.paths.get_active_with_pid() {
            if from.is_some() && Some(p.local_addr()) != from {
                return Err(Error::Done);
            }

            if to.is_some() && Some(p.peer_addr()) != to {
                return Err(Error::Done);
            }

            return Ok(pid);
        };

        Err(Error::InvalidState)
    }

    /// Sets the path with identifier 'path_id' to be active.
    fn set_active_path(
        &mut self, path_id: usize, now: time::Instant,
    ) -> Result<()> {
        if let Ok(old_active_path) = self.paths.get_active_mut() {
            for &e in packet::Epoch::epochs(
                packet::Epoch::Initial..=packet::Epoch::Application,
            ) {
                let (lost_packets, lost_bytes) = old_active_path
                    .recovery
                    .on_path_change(e, now, &self.trace_id);

                self.lost_count += lost_packets;
                self.lost_bytes += lost_bytes as u64;
            }
        }

        self.paths.set_active_path(path_id)
    }

    /// Handles potential connection migration.
    fn on_peer_migrated(
        &mut self, new_pid: usize, disable_dcid_reuse: bool, now: time::Instant,
    ) -> Result<()> {
        let active_path_id = self.paths.get_active_path_id()?;

        if active_path_id == new_pid {
            return Ok(());
        }

        self.set_active_path(new_pid, now)?;

        let no_spare_dcid =
            self.paths.get_mut(new_pid)?.active_dcid_seq.is_none();

        if no_spare_dcid && !disable_dcid_reuse {
            self.paths.get_mut(new_pid)?.active_dcid_seq =
                self.paths.get_mut(active_path_id)?.active_dcid_seq;
        }

        Ok(())
    }

    /// Creates a new client-side path.
    fn create_path_on_client(
        &mut self, local_addr: SocketAddr, peer_addr: SocketAddr,
    ) -> Result<usize> {
        if self.is_server {
            return Err(Error::InvalidState);
        }

        // If we use zero-length SCID and go over our local active CID limit,
        // the `insert_path()` call will raise an error.
        if !self.ids.zero_length_scid() && self.ids.available_scids() == 0 {
            return Err(Error::OutOfIdentifiers);
        }

        // Do we have a spare DCID? If we are using zero-length DCID, just use
        // the default having sequence 0 (note that if we exceed our local CID
        // limit, the `insert_path()` call will raise an error.
        let dcid_seq = if self.ids.zero_length_dcid() {
            0
        } else {
            self.ids
                .lowest_available_dcid_seq()
                .ok_or(Error::OutOfIdentifiers)?
        };

        let mut path = path::Path::new(
            local_addr,
            peer_addr,
            &self.recovery_config,
            self.path_challenge_recv_max_queue_len,
            MIN_CLIENT_INITIAL_LEN,
            false,
        );
        path.active_dcid_seq = Some(dcid_seq);

        let pid = self
            .paths
            .insert_path(path, false)
            .map_err(|_| Error::OutOfIdentifiers)?;
        self.ids.link_dcid_to_path_id(dcid_seq, pid)?;

        Ok(pid)
    }

    // Marks the connection as closed and does any related tidyup.
    fn mark_closed(&mut self) {
        #[cfg(feature = "qlog")]
        {
            let cc = match (self.is_established(), self.timed_out, &self.peer_error, &self.local_error) {
                (false, _, _, _) => qlog::events::connectivity::ConnectionClosed {
                    owner: Some(TransportOwner::Local),
                    connection_code: None,
                    application_code: None,
                    internal_code: None,
                    reason: Some("Failed to establish connection".to_string()),
                    trigger: Some(qlog::events::connectivity::ConnectionClosedTrigger::HandshakeTimeout)
                },

                (true, true, _, _) => qlog::events::connectivity::ConnectionClosed {
                    owner: Some(TransportOwner::Local),
                    connection_code: None,
                    application_code: None,
                    internal_code: None,
                    reason: Some("Idle timeout".to_string()),
                    trigger: Some(qlog::events::connectivity::ConnectionClosedTrigger::IdleTimeout)
                },

                (true, false, Some(peer_error), None) => {
                    let (connection_code, application_code) = if peer_error.is_app {
                        (None, Some(qlog::events::ApplicationErrorCode::Value(peer_error.error_code)))
                    } else {
                        (Some(qlog::events::ConnectionErrorCode::Value(peer_error.error_code)), None)
                    };

                    qlog::events::connectivity::ConnectionClosed {
                        owner: Some(TransportOwner::Remote),
                        connection_code,
                        application_code,
                        internal_code: None,
                        reason: Some(String::from_utf8_lossy(&peer_error.reason).to_string()),
                        trigger: Some(qlog::events::connectivity::ConnectionClosedTrigger::Error),
                    }
                },

                (true, false, None, Some(local_error)) => {
                    let (connection_code, application_code) = if local_error.is_app {
                        (None, Some(qlog::events::ApplicationErrorCode::Value(local_error.error_code)))
                    } else {
                        (Some(qlog::events::ConnectionErrorCode::Value(local_error.error_code)), None)
                    };

                    qlog::events::connectivity::ConnectionClosed {
                        owner: Some(TransportOwner::Local),
                        connection_code,
                        application_code,
                        internal_code: None,
                        reason: Some(String::from_utf8_lossy(&local_error.reason).to_string()),
                        trigger: Some(qlog::events::connectivity::ConnectionClosedTrigger::Error),
                    }
                },

                _ => qlog::events::connectivity::ConnectionClosed {
                    owner: None,
                    connection_code: None,
                    application_code: None,
                    internal_code: None,
                    reason: None,
                    trigger: None,
                },
            };

            qlog_with_type!(QLOG_CONNECTION_CLOSED, self.qlog, q, {
                let ev_data = qlog::events::EventData::ConnectionClosed(cc);

                q.add_event_data_now(ev_data).ok();
            });
            self.qlog.streamer = None;
        }
        self.closed = true;
    }
}

#[cfg(feature = "boringssl-boring-crate")]
impl AsMut<boring::ssl::SslRef> for Connection {
    fn as_mut(&mut self) -> &mut boring::ssl::SslRef {
        self.handshake.ssl_mut()
    }
}

/// Maps an `Error` to `Error::Done`, or itself.
///
/// When a received packet that hasn't yet been authenticated triggers a failure
/// it should, in most cases, be ignored, instead of raising a connection error,
/// to avoid potential man-in-the-middle and man-on-the-side attacks.
///
/// However, if no other packet was previously received, the connection should
/// indeed be closed as the received packet might just be network background
/// noise, and it shouldn't keep resources occupied indefinitely.
///
/// This function maps an error to `Error::Done` to ignore a packet failure
/// without aborting the connection, except when no other packet was previously
/// received, in which case the error itself is returned, but only on the
/// server-side as the client will already have armed the idle timer.
///
/// This must only be used for errors preceding packet authentication. Failures
/// happening after a packet has been authenticated should still cause the
/// connection to be aborted.
fn drop_pkt_on_err(
    e: Error, recv_count: usize, is_server: bool, trace_id: &str,
) -> Error {
    // On the server, if no other packet has been successfully processed, abort
    // the connection to avoid keeping the connection open when only junk is
    // received.
    if is_server && recv_count == 0 {
        return e;
    }

    trace!("{} dropped invalid packet", trace_id);

    // Ignore other invalid packets that haven't been authenticated to prevent
    // man-in-the-middle and man-on-the-side attacks.
    Error::Done
}

struct AddrTupleFmt(SocketAddr, SocketAddr);

impl std::fmt::Display for AddrTupleFmt {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let AddrTupleFmt(src, dst) = &self;

        if src.ip().is_unspecified() || dst.ip().is_unspecified() {
            return Ok(());
        }

        f.write_fmt(format_args!("src:{src} dst:{dst}"))
    }
}

/// Statistics about the connection.
///
/// A connection's statistics can be collected using the [`stats()`] method.
///
/// [`stats()`]: struct.Connection.html#method.stats
#[derive(Clone, Default)]
pub struct Stats {
    /// The number of QUIC packets received.
    pub recv: usize,

    /// The number of QUIC packets sent.
    pub sent: usize,

    /// The number of QUIC packets that were lost.
    pub lost: usize,

    /// The number of sent QUIC packets with retransmitted data.
    pub retrans: usize,

    /// The number of sent bytes.
    pub sent_bytes: u64,

    /// The number of received bytes.
    pub recv_bytes: u64,

    /// The number of bytes sent acked.
    pub acked_bytes: u64,

    /// The number of bytes sent lost.
    pub lost_bytes: u64,

    /// The number of stream bytes retransmitted.
    pub stream_retrans_bytes: u64,

    /// The number of known paths for the connection.
    pub paths_count: usize,

    /// The number of streams reset by local.
    pub reset_stream_count_local: u64,

    /// The number of streams stopped by local.
    pub stopped_stream_count_local: u64,

    /// The number of streams reset by remote.
    pub reset_stream_count_remote: u64,

    /// The number of streams stopped by remote.
    pub stopped_stream_count_remote: u64,

    /// The total number of PATH_CHALLENGE frames that were received.
    pub path_challenge_rx_count: u64,
}

impl std::fmt::Debug for Stats {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "recv={} sent={} lost={} retrans={}",
            self.recv, self.sent, self.lost, self.retrans,
        )?;

        write!(
            f,
            " sent_bytes={} recv_bytes={} lost_bytes={}",
            self.sent_bytes, self.recv_bytes, self.lost_bytes,
        )?;

        Ok(())
    }
}

/// QUIC Transport Parameters
#[derive(Clone, Debug, PartialEq)]
pub struct TransportParams {
    /// Value of Destination CID field from first Initial packet sent by client
    pub original_destination_connection_id: Option<ConnectionId<'static>>,
    /// The maximum idle timeout.
    pub max_idle_timeout: u64,
    /// Token used for verifying stateless resets
    pub stateless_reset_token: Option<u128>,
    /// The maximum UDP payload size.
    pub max_udp_payload_size: u64,
    /// The initial flow control maximum data for the connection.
    pub initial_max_data: u64,
    /// The initial flow control maximum data for local bidirectional streams.
    pub initial_max_stream_data_bidi_local: u64,
    /// The initial flow control maximum data for remote bidirectional streams.
    pub initial_max_stream_data_bidi_remote: u64,
    /// The initial flow control maximum data for unidirectional streams.
    pub initial_max_stream_data_uni: u64,
    /// The initial maximum bidirectional streams.
    pub initial_max_streams_bidi: u64,
    /// The initial maximum unidirectional streams.
    pub initial_max_streams_uni: u64,
    /// The ACK delay exponent.
    pub ack_delay_exponent: u64,
    /// The max ACK delay.
    pub max_ack_delay: u64,
    /// Whether active migration is disabled.
    pub disable_active_migration: bool,
    /// The active connection ID limit.
    pub active_conn_id_limit: u64,
    /// The value that the endpoint included in the Source CID field of a Retry
    /// Packet.
    pub initial_source_connection_id: Option<ConnectionId<'static>>,
    /// The value that the server included in the Source CID field of a Retry
    /// Packet.
    pub retry_source_connection_id: Option<ConnectionId<'static>>,
    /// DATAGRAM frame extension parameter, if any.
    pub max_datagram_frame_size: Option<u64>,
    // pub preferred_address: ...,
}

impl Default for TransportParams {
    fn default() -> TransportParams {
        TransportParams {
            original_destination_connection_id: None,
            max_idle_timeout: 0,
            stateless_reset_token: None,
            max_udp_payload_size: 65527,
            initial_max_data: 0,
            initial_max_stream_data_bidi_local: 0,
            initial_max_stream_data_bidi_remote: 0,
            initial_max_stream_data_uni: 0,
            initial_max_streams_bidi: 0,
            initial_max_streams_uni: 0,
            ack_delay_exponent: 3,
            max_ack_delay: 25,
            disable_active_migration: false,
            active_conn_id_limit: 2,
            initial_source_connection_id: None,
            retry_source_connection_id: None,
            max_datagram_frame_size: None,
        }
    }
}

impl TransportParams {
    fn decode(buf: &[u8], is_server: bool) -> Result<TransportParams> {
        let mut params = octets::Octets::with_slice(buf);
        let mut seen_params = HashSet::new();

        let mut tp = TransportParams::default();

        while params.cap() > 0 {
            let id = params.get_varint()?;

            if seen_params.contains(&id) {
                return Err(Error::InvalidTransportParam);
            }
            seen_params.insert(id);

            let mut val = params.get_bytes_with_varint_length()?;

            match id {
                0x0000 => {
                    if is_server {
                        return Err(Error::InvalidTransportParam);
                    }

                    tp.original_destination_connection_id =
                        Some(val.to_vec().into());
                },

                0x0001 => {
                    tp.max_idle_timeout = val.get_varint()?;
                },

                0x0002 => {
                    if is_server {
                        return Err(Error::InvalidTransportParam);
                    }

                    tp.stateless_reset_token = Some(u128::from_be_bytes(
                        val.get_bytes(16)?
                            .to_vec()
                            .try_into()
                            .map_err(|_| Error::BufferTooShort)?,
                    ));
                },

                0x0003 => {
                    tp.max_udp_payload_size = val.get_varint()?;

                    if tp.max_udp_payload_size < 1200 {
                        return Err(Error::InvalidTransportParam);
                    }
                },

                0x0004 => {
                    tp.initial_max_data = val.get_varint()?;
                },

                0x0005 => {
                    tp.initial_max_stream_data_bidi_local = val.get_varint()?;
                },

                0x0006 => {
                    tp.initial_max_stream_data_bidi_remote = val.get_varint()?;
                },

                0x0007 => {
                    tp.initial_max_stream_data_uni = val.get_varint()?;
                },

                0x0008 => {
                    let max = val.get_varint()?;

                    if max > MAX_STREAM_ID {
                        return Err(Error::InvalidTransportParam);
                    }

                    tp.initial_max_streams_bidi = max;
                },

                0x0009 => {
                    let max = val.get_varint()?;

                    if max > MAX_STREAM_ID {
                        return Err(Error::InvalidTransportParam);
                    }

                    tp.initial_max_streams_uni = max;
                },

                0x000a => {
                    let ack_delay_exponent = val.get_varint()?;

                    if ack_delay_exponent > 20 {
                        return Err(Error::InvalidTransportParam);
                    }

                    tp.ack_delay_exponent = ack_delay_exponent;
                },

                0x000b => {
                    let max_ack_delay = val.get_varint()?;

                    if max_ack_delay >= 2_u64.pow(14) {
                        return Err(Error::InvalidTransportParam);
                    }

                    tp.max_ack_delay = max_ack_delay;
                },

                0x000c => {
                    tp.disable_active_migration = true;
                },

                0x000d => {
                    if is_server {
                        return Err(Error::InvalidTransportParam);
                    }

                    // TODO: decode preferred_address
                },

                0x000e => {
                    let limit = val.get_varint()?;

                    if limit < 2 {
                        return Err(Error::InvalidTransportParam);
                    }

                    tp.active_conn_id_limit = limit;
                },

                0x000f => {
                    tp.initial_source_connection_id = Some(val.to_vec().into());
                },

                0x00010 => {
                    if is_server {
                        return Err(Error::InvalidTransportParam);
                    }

                    tp.retry_source_connection_id = Some(val.to_vec().into());
                },

                0x0020 => {
                    tp.max_datagram_frame_size = Some(val.get_varint()?);
                },

                // Ignore unknown parameters.
                _ => (),
            }
        }

        Ok(tp)
    }

    fn encode_param(
        b: &mut octets::OctetsMut, ty: u64, len: usize,
    ) -> Result<()> {
        b.put_varint(ty)?;
        b.put_varint(len as u64)?;

        Ok(())
    }

    fn encode<'a>(
        tp: &TransportParams, is_server: bool, out: &'a mut [u8],
    ) -> Result<&'a mut [u8]> {
        let mut b = octets::OctetsMut::with_slice(out);

        if is_server {
            if let Some(ref odcid) = tp.original_destination_connection_id {
                TransportParams::encode_param(&mut b, 0x0000, odcid.len())?;
                b.put_bytes(odcid)?;
            }
        };

        if tp.max_idle_timeout != 0 {
            TransportParams::encode_param(
                &mut b,
                0x0001,
                octets::varint_len(tp.max_idle_timeout),
            )?;
            b.put_varint(tp.max_idle_timeout)?;
        }

        if is_server {
            if let Some(ref token) = tp.stateless_reset_token {
                TransportParams::encode_param(&mut b, 0x0002, 16)?;
                b.put_bytes(&token.to_be_bytes())?;
            }
        }

        if tp.max_udp_payload_size != 0 {
            TransportParams::encode_param(
                &mut b,
                0x0003,
                octets::varint_len(tp.max_udp_payload_size),
            )?;
            b.put_varint(tp.max_udp_payload_size)?;
        }

        if tp.initial_max_data != 0 {
            TransportParams::encode_param(
                &mut b,
                0x0004,
                octets::varint_len(tp.initial_max_data),
            )?;
            b.put_varint(tp.initial_max_data)?;
        }

        if tp.initial_max_stream_data_bidi_local != 0 {
            TransportParams::encode_param(
                &mut b,
                0x0005,
                octets::varint_len(tp.initial_max_stream_data_bidi_local),
            )?;
            b.put_varint(tp.initial_max_stream_data_bidi_local)?;
        }

        if tp.initial_max_stream_data_bidi_remote != 0 {
            TransportParams::encode_param(
                &mut b,
                0x0006,
                octets::varint_len(tp.initial_max_stream_data_bidi_remote),
            )?;
            b.put_varint(tp.initial_max_stream_data_bidi_remote)?;
        }

        if tp.initial_max_stream_data_uni != 0 {
            TransportParams::encode_param(
                &mut b,
                0x0007,
                octets::varint_len(tp.initial_max_stream_data_uni),
            )?;
            b.put_varint(tp.initial_max_stream_data_uni)?;
        }

        if tp.initial_max_streams_bidi != 0 {
            TransportParams::encode_param(
                &mut b,
                0x0008,
                octets::varint_len(tp.initial_max_streams_bidi),
            )?;
            b.put_varint(tp.initial_max_streams_bidi)?;
        }

        if tp.initial_max_streams_uni != 0 {
            TransportParams::encode_param(
                &mut b,
                0x0009,
                octets::varint_len(tp.initial_max_streams_uni),
            )?;
            b.put_varint(tp.initial_max_streams_uni)?;
        }

        if tp.ack_delay_exponent != 0 {
            TransportParams::encode_param(
                &mut b,
                0x000a,
                octets::varint_len(tp.ack_delay_exponent),
            )?;
            b.put_varint(tp.ack_delay_exponent)?;
        }

        if tp.max_ack_delay != 0 {
            TransportParams::encode_param(
                &mut b,
                0x000b,
                octets::varint_len(tp.max_ack_delay),
            )?;
            b.put_varint(tp.max_ack_delay)?;
        }

        if tp.disable_active_migration {
            TransportParams::encode_param(&mut b, 0x000c, 0)?;
        }

        // TODO: encode preferred_address

        if tp.active_conn_id_limit != 2 {
            TransportParams::encode_param(
                &mut b,
                0x000e,
                octets::varint_len(tp.active_conn_id_limit),
            )?;
            b.put_varint(tp.active_conn_id_limit)?;
        }

        if let Some(scid) = &tp.initial_source_connection_id {
            TransportParams::encode_param(&mut b, 0x000f, scid.len())?;
            b.put_bytes(scid)?;
        }

        if is_server {
            if let Some(scid) = &tp.retry_source_connection_id {
                TransportParams::encode_param(&mut b, 0x0010, scid.len())?;
                b.put_bytes(scid)?;
            }
        }

        if let Some(max_datagram_frame_size) = tp.max_datagram_frame_size {
            TransportParams::encode_param(
                &mut b,
                0x0020,
                octets::varint_len(max_datagram_frame_size),
            )?;
            b.put_varint(max_datagram_frame_size)?;
        }

        let out_len = b.off();

        Ok(&mut out[..out_len])
    }

    /// Creates a qlog event for connection transport parameters and TLS fields
    #[cfg(feature = "qlog")]
    pub fn to_qlog(
        &self, owner: TransportOwner, cipher: Option<crypto::Algorithm>,
    ) -> EventData {
        let original_destination_connection_id = qlog::HexSlice::maybe_string(
            self.original_destination_connection_id.as_ref(),
        );

        let stateless_reset_token = qlog::HexSlice::maybe_string(
            self.stateless_reset_token.map(|s| s.to_be_bytes()).as_ref(),
        );

        EventData::TransportParametersSet(
            qlog::events::quic::TransportParametersSet {
                owner: Some(owner),
                resumption_allowed: None,
                early_data_enabled: None,
                tls_cipher: Some(format!("{cipher:?}")),
                aead_tag_length: None,
                original_destination_connection_id,
                initial_source_connection_id: None,
                retry_source_connection_id: None,
                stateless_reset_token,
                disable_active_migration: Some(self.disable_active_migration),
                max_idle_timeout: Some(self.max_idle_timeout),
                max_udp_payload_size: Some(self.max_udp_payload_size as u32),
                ack_delay_exponent: Some(self.ack_delay_exponent as u16),
                max_ack_delay: Some(self.max_ack_delay as u16),
                active_connection_id_limit: Some(
                    self.active_conn_id_limit as u32,
                ),

                initial_max_data: Some(self.initial_max_data),
                initial_max_stream_data_bidi_local: Some(
                    self.initial_max_stream_data_bidi_local,
                ),
                initial_max_stream_data_bidi_remote: Some(
                    self.initial_max_stream_data_bidi_remote,
                ),
                initial_max_stream_data_uni: Some(
                    self.initial_max_stream_data_uni,
                ),
                initial_max_streams_bidi: Some(self.initial_max_streams_bidi),
                initial_max_streams_uni: Some(self.initial_max_streams_uni),

                preferred_address: None,
            },
        )
    }
}

#[doc(hidden)]
pub mod testing {
    use super::*;

    pub struct Pipe {
        pub client: Connection,
        pub server: Connection,
    }

    impl Pipe {
        pub fn new() -> Result<Pipe> {
            let mut config = Config::new(crate::PROTOCOL_VERSION)?;
            config.load_cert_chain_from_pem_file("examples/cert.crt")?;
            config.load_priv_key_from_pem_file("examples/cert.key")?;
            config.set_application_protos(&[b"proto1", b"proto2"])?;
            config.set_initial_max_data(30);
            config.set_initial_max_stream_data_bidi_local(15);
            config.set_initial_max_stream_data_bidi_remote(15);
            config.set_initial_max_stream_data_uni(10);
            config.set_initial_max_streams_bidi(3);
            config.set_initial_max_streams_uni(3);
            config.set_max_idle_timeout(180_000);
            config.verify_peer(false);
            config.set_ack_delay_exponent(8);

            Pipe::with_config(&mut config)
        }

        pub fn client_addr() -> SocketAddr {
            "127.0.0.1:1234".parse().unwrap()
        }

        pub fn server_addr() -> SocketAddr {
            "127.0.0.1:4321".parse().unwrap()
        }

        pub fn with_config(config: &mut Config) -> Result<Pipe> {
            let mut client_scid = [0; 16];
            rand::rand_bytes(&mut client_scid[..]);
            let client_scid = ConnectionId::from_ref(&client_scid);
            let client_addr = Pipe::client_addr();

            let mut server_scid = [0; 16];
            rand::rand_bytes(&mut server_scid[..]);
            let server_scid = ConnectionId::from_ref(&server_scid);
            let server_addr = Pipe::server_addr();

            Ok(Pipe {
                client: connect(
                    Some("quic.tech"),
                    &client_scid,
                    client_addr,
                    server_addr,
                    config,
                )?,
                server: accept(
                    &server_scid,
                    None,
                    server_addr,
                    client_addr,
                    config,
                )?,
            })
        }

        pub fn with_config_and_scid_lengths(
            config: &mut Config, client_scid_len: usize, server_scid_len: usize,
        ) -> Result<Pipe> {
            let mut client_scid = vec![0; client_scid_len];
            rand::rand_bytes(&mut client_scid[..]);
            let client_scid = ConnectionId::from_ref(&client_scid);
            let client_addr = Pipe::client_addr();

            let mut server_scid = vec![0; server_scid_len];
            rand::rand_bytes(&mut server_scid[..]);
            let server_scid = ConnectionId::from_ref(&server_scid);
            let server_addr = Pipe::server_addr();

            Ok(Pipe {
                client: connect(
                    Some("quic.tech"),
                    &client_scid,
                    client_addr,
                    server_addr,
                    config,
                )?,
                server: accept(
                    &server_scid,
                    None,
                    server_addr,
                    client_addr,
                    config,
                )?,
            })
        }

        pub fn with_client_config(client_config: &mut Config) -> Result<Pipe> {
            let mut client_scid = [0; 16];
            rand::rand_bytes(&mut client_scid[..]);
            let client_scid = ConnectionId::from_ref(&client_scid);
            let client_addr = Pipe::client_addr();

            let mut server_scid = [0; 16];
            rand::rand_bytes(&mut server_scid[..]);
            let server_scid = ConnectionId::from_ref(&server_scid);
            let server_addr = Pipe::server_addr();

            let mut config = Config::new(crate::PROTOCOL_VERSION)?;
            config.load_cert_chain_from_pem_file("examples/cert.crt")?;
            config.load_priv_key_from_pem_file("examples/cert.key")?;
            config.set_application_protos(&[b"proto1", b"proto2"])?;
            config.set_initial_max_data(30);
            config.set_initial_max_stream_data_bidi_local(15);
            config.set_initial_max_stream_data_bidi_remote(15);
            config.set_initial_max_streams_bidi(3);
            config.set_initial_max_streams_uni(3);
            config.set_ack_delay_exponent(8);

            Ok(Pipe {
                client: connect(
                    Some("quic.tech"),
                    &client_scid,
                    client_addr,
                    server_addr,
                    client_config,
                )?,
                server: accept(
                    &server_scid,
                    None,
                    server_addr,
                    client_addr,
                    &mut config,
                )?,
            })
        }

        pub fn with_server_config(server_config: &mut Config) -> Result<Pipe> {
            let mut client_scid = [0; 16];
            rand::rand_bytes(&mut client_scid[..]);
            let client_scid = ConnectionId::from_ref(&client_scid);
            let client_addr = Pipe::client_addr();

            let mut server_scid = [0; 16];
            rand::rand_bytes(&mut server_scid[..]);
            let server_scid = ConnectionId::from_ref(&server_scid);
            let server_addr = Pipe::server_addr();

            let mut config = Config::new(crate::PROTOCOL_VERSION)?;
            config.set_application_protos(&[b"proto1", b"proto2"])?;
            config.set_initial_max_data(30);
            config.set_initial_max_stream_data_bidi_local(15);
            config.set_initial_max_stream_data_bidi_remote(15);
            config.set_initial_max_streams_bidi(3);
            config.set_initial_max_streams_uni(3);
            config.set_ack_delay_exponent(8);

            Ok(Pipe {
                client: connect(
                    Some("quic.tech"),
                    &client_scid,
                    client_addr,
                    server_addr,
                    &mut config,
                )?,
                server: accept(
                    &server_scid,
                    None,
                    server_addr,
                    client_addr,
                    server_config,
                )?,
            })
        }

        pub fn with_client_and_server_config(
            client_config: &mut Config, server_config: &mut Config,
        ) -> Result<Pipe> {
            let mut client_scid = [0; 16];
            rand::rand_bytes(&mut client_scid[..]);
            let client_scid = ConnectionId::from_ref(&client_scid);
            let client_addr = Pipe::client_addr();

            let mut server_scid = [0; 16];
            rand::rand_bytes(&mut server_scid[..]);
            let server_scid = ConnectionId::from_ref(&server_scid);
            let server_addr = Pipe::server_addr();

            Ok(Pipe {
                client: connect(
                    Some("quic.tech"),
                    &client_scid,
                    client_addr,
                    server_addr,
                    client_config,
                )?,
                server: accept(
                    &server_scid,
                    None,
                    server_addr,
                    client_addr,
                    server_config,
                )?,
            })
        }

        pub fn handshake(&mut self) -> Result<()> {
            while !self.client.is_established() || !self.server.is_established() {
                let flight = emit_flight(&mut self.client)?;
                process_flight(&mut self.server, flight)?;

                let flight = emit_flight(&mut self.server)?;
                process_flight(&mut self.client, flight)?;
            }

            Ok(())
        }

        pub fn advance(&mut self) -> Result<()> {
            let mut client_done = false;
            let mut server_done = false;

            while !client_done || !server_done {
                match emit_flight(&mut self.client) {
                    Ok(flight) => process_flight(&mut self.server, flight)?,

                    Err(Error::Done) => client_done = true,

                    Err(e) => return Err(e),
                };

                match emit_flight(&mut self.server) {
                    Ok(flight) => process_flight(&mut self.client, flight)?,

                    Err(Error::Done) => server_done = true,

                    Err(e) => return Err(e),
                };
            }

            Ok(())
        }

        pub fn client_recv(&mut self, buf: &mut [u8]) -> Result<usize> {
            let server_path = &self.server.paths.get_active().unwrap();
            let info = RecvInfo {
                to: server_path.peer_addr(),
                from: server_path.local_addr(),
            };

            self.client.recv(buf, info)
        }

        pub fn server_recv(&mut self, buf: &mut [u8]) -> Result<usize> {
            let client_path = &self.client.paths.get_active().unwrap();
            let info = RecvInfo {
                to: client_path.peer_addr(),
                from: client_path.local_addr(),
            };

            self.server.recv(buf, info)
        }

        pub fn send_pkt_to_server(
            &mut self, pkt_type: packet::Type, frames: &[frame::Frame],
            buf: &mut [u8],
        ) -> Result<usize> {
            let written = encode_pkt(&mut self.client, pkt_type, frames, buf)?;
            recv_send(&mut self.server, buf, written)
        }

        pub fn client_update_key(&mut self) -> Result<()> {
            let space =
                &mut self.client.pkt_num_spaces[packet::Epoch::Application];

            let open_next = space
                .crypto_open
                .as_ref()
                .unwrap()
                .derive_next_packet_key()
                .unwrap();

            let seal_next = space
                .crypto_seal
                .as_ref()
                .unwrap()
                .derive_next_packet_key()?;

            let open_prev = space.crypto_open.replace(open_next);
            space.crypto_seal.replace(seal_next);

            space.key_update = Some(packet::KeyUpdate {
                crypto_open: open_prev.unwrap(),
                pn_on_update: space.next_pkt_num,
                update_acked: true,
                timer: time::Instant::now(),
            });

            self.client.key_phase = !self.client.key_phase;

            Ok(())
        }
    }

    pub fn recv_send(
        conn: &mut Connection, buf: &mut [u8], len: usize,
    ) -> Result<usize> {
        let active_path = conn.paths.get_active()?;
        let info = RecvInfo {
            to: active_path.local_addr(),
            from: active_path.peer_addr(),
        };

        conn.recv(&mut buf[..len], info)?;

        let mut off = 0;

        match conn.send(&mut buf[off..]) {
            Ok((write, _)) => off += write,

            Err(Error::Done) => (),

            Err(e) => return Err(e),
        }

        Ok(off)
    }

    pub fn process_flight(
        conn: &mut Connection, flight: Vec<(Vec<u8>, SendInfo)>,
    ) -> Result<()> {
        for (mut pkt, si) in flight {
            let info = RecvInfo {
                to: si.to,
                from: si.from,
            };

            conn.recv(&mut pkt, info)?;
        }

        Ok(())
    }

    pub fn emit_flight_with_max_buffer(
        conn: &mut Connection, out_size: usize, from: Option<SocketAddr>,
        to: Option<SocketAddr>,
    ) -> Result<Vec<(Vec<u8>, SendInfo)>> {
        let mut flight = Vec::new();

        loop {
            let mut out = vec![0u8; out_size];

            let info = match conn.send_on_path(&mut out, from, to) {
                Ok((written, info)) => {
                    out.truncate(written);
                    info
                },

                Err(Error::Done) => break,

                Err(e) => return Err(e),
            };

            flight.push((out, info));
        }

        if flight.is_empty() {
            return Err(Error::Done);
        }

        Ok(flight)
    }

    pub fn emit_flight_on_path(
        conn: &mut Connection, from: Option<SocketAddr>, to: Option<SocketAddr>,
    ) -> Result<Vec<(Vec<u8>, SendInfo)>> {
        emit_flight_with_max_buffer(conn, 65535, from, to)
    }

    pub fn emit_flight(
        conn: &mut Connection,
    ) -> Result<Vec<(Vec<u8>, SendInfo)>> {
        emit_flight_on_path(conn, None, None)
    }

    pub fn encode_pkt(
        conn: &mut Connection, pkt_type: packet::Type, frames: &[frame::Frame],
        buf: &mut [u8],
    ) -> Result<usize> {
        let mut b = octets::OctetsMut::with_slice(buf);

        let epoch = pkt_type.to_epoch()?;

        let space = &mut conn.pkt_num_spaces[epoch];

        let pn = space.next_pkt_num;
        let pn_len = 4;

        let send_path = conn.paths.get_active()?;
        let active_dcid_seq = send_path
            .active_dcid_seq
            .as_ref()
            .ok_or(Error::InvalidState)?;
        let active_scid_seq = send_path
            .active_scid_seq
            .as_ref()
            .ok_or(Error::InvalidState)?;

        let hdr = Header {
            ty: pkt_type,
            version: conn.version,
            dcid: ConnectionId::from_ref(
                conn.ids.get_dcid(*active_dcid_seq)?.cid.as_ref(),
            ),
            scid: ConnectionId::from_ref(
                conn.ids.get_scid(*active_scid_seq)?.cid.as_ref(),
            ),
            pkt_num: pn,
            pkt_num_len: pn_len,
            token: conn.token.clone(),
            versions: None,
            key_phase: conn.key_phase,
        };

        hdr.to_bytes(&mut b)?;

        let payload_len = frames.iter().fold(0, |acc, x| acc + x.wire_len());

        if pkt_type != packet::Type::Short {
            let len = pn_len + payload_len + space.crypto_overhead().unwrap();
            b.put_varint(len as u64)?;
        }

        // Always encode packet number in 4 bytes, to allow encoding packets
        // with empty payloads.
        b.put_u32(pn as u32)?;

        let payload_offset = b.off();

        for frame in frames {
            frame.to_bytes(&mut b)?;
        }

        let aead = match space.crypto_seal {
            Some(ref v) => v,
            None => return Err(Error::InvalidState),
        };

        let written = packet::encrypt_pkt(
            &mut b,
            pn,
            pn_len,
            payload_len,
            payload_offset,
            None,
            aead,
        )?;

        space.next_pkt_num += 1;

        Ok(written)
    }

    pub fn decode_pkt(
        conn: &mut Connection, buf: &mut [u8],
    ) -> Result<Vec<frame::Frame>> {
        let mut b = octets::OctetsMut::with_slice(buf);

        let mut hdr = Header::from_bytes(&mut b, conn.source_id().len()).unwrap();

        let epoch = hdr.ty.to_epoch()?;

        let aead = conn.pkt_num_spaces[epoch].crypto_open.as_ref().unwrap();

        let payload_len = b.cap();

        packet::decrypt_hdr(&mut b, &mut hdr, aead).unwrap();

        let pn = packet::decode_pkt_num(
            conn.pkt_num_spaces[epoch].largest_rx_pkt_num,
            hdr.pkt_num,
            hdr.pkt_num_len,
        );

        let mut payload =
            packet::decrypt_pkt(&mut b, pn, hdr.pkt_num_len, payload_len, aead)
                .unwrap();

        let mut frames = Vec::new();

        while payload.cap() > 0 {
            let frame = frame::Frame::from_bytes(&mut payload, hdr.ty)?;
            frames.push(frame);
        }

        Ok(frames)
    }

    pub fn create_cid_and_reset_token(
        cid_len: usize,
    ) -> (ConnectionId<'static>, u128) {
        let mut cid = vec![0; cid_len];
        rand::rand_bytes(&mut cid[..]);
        let cid = ConnectionId::from_ref(&cid).into_owned();

        let mut reset_token = [0; 16];
        rand::rand_bytes(&mut reset_token);
        let reset_token = u128::from_be_bytes(reset_token);

        (cid, reset_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_params() {
        // Server encodes, client decodes.
        let tp = TransportParams {
            original_destination_connection_id: None,
            max_idle_timeout: 30,
            stateless_reset_token: Some(u128::from_be_bytes([0xba; 16])),
            max_udp_payload_size: 23_421,
            initial_max_data: 424_645_563,
            initial_max_stream_data_bidi_local: 154_323_123,
            initial_max_stream_data_bidi_remote: 6_587_456,
            initial_max_stream_data_uni: 2_461_234,
            initial_max_streams_bidi: 12_231,
            initial_max_streams_uni: 18_473,
            ack_delay_exponent: 20,
            max_ack_delay: 2_u64.pow(14) - 1,
            disable_active_migration: true,
            active_conn_id_limit: 8,
            initial_source_connection_id: Some(b"woot woot".to_vec().into()),
            retry_source_connection_id: Some(b"retry".to_vec().into()),
            max_datagram_frame_size: Some(32),
        };

        let mut raw_params = [42; 256];
        let raw_params =
            TransportParams::encode(&tp, true, &mut raw_params).unwrap();
        assert_eq!(raw_params.len(), 94);

        let new_tp = TransportParams::decode(raw_params, false).unwrap();

        assert_eq!(new_tp, tp);

        // Client encodes, server decodes.
        let tp = TransportParams {
            original_destination_connection_id: None,
            max_idle_timeout: 30,
            stateless_reset_token: None,
            max_udp_payload_size: 23_421,
            initial_max_data: 424_645_563,
            initial_max_stream_data_bidi_local: 154_323_123,
            initial_max_stream_data_bidi_remote: 6_587_456,
            initial_max_stream_data_uni: 2_461_234,
            initial_max_streams_bidi: 12_231,
            initial_max_streams_uni: 18_473,
            ack_delay_exponent: 20,
            max_ack_delay: 2_u64.pow(14) - 1,
            disable_active_migration: true,
            active_conn_id_limit: 8,
            initial_source_connection_id: Some(b"woot woot".to_vec().into()),
            retry_source_connection_id: None,
            max_datagram_frame_size: Some(32),
        };

        let mut raw_params = [42; 256];
        let raw_params =
            TransportParams::encode(&tp, false, &mut raw_params).unwrap();
        assert_eq!(raw_params.len(), 69);

        let new_tp = TransportParams::decode(raw_params, true).unwrap();

        assert_eq!(new_tp, tp);
    }

    #[test]
    fn transport_params_forbid_duplicates() {
        // Given an encoded param.
        let initial_source_connection_id = b"id";
        let initial_source_connection_id_raw = [
            15,
            initial_source_connection_id.len() as u8,
            initial_source_connection_id[0],
            initial_source_connection_id[1],
        ];

        // No error when decoding the param.
        let tp = TransportParams::decode(
            initial_source_connection_id_raw.as_slice(),
            true,
        )
        .unwrap();

        assert_eq!(
            tp.initial_source_connection_id,
            Some(initial_source_connection_id.to_vec().into())
        );

        // Duplicate the param.
        let mut raw_params = Vec::new();
        raw_params.append(&mut initial_source_connection_id_raw.to_vec());
        raw_params.append(&mut initial_source_connection_id_raw.to_vec());

        // Decoding fails.
        assert_eq!(
            TransportParams::decode(raw_params.as_slice(), true),
            Err(Error::InvalidTransportParam)
        );
    }

    #[test]
    fn unknown_version() {
        let mut config = Config::new(0xbabababa).unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_client_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Err(Error::UnknownVersion));
    }

    #[test]
    fn config_version_reserved() {
        Config::new(0xbabababa).unwrap();
        Config::new(0x1a2a3a4a).unwrap();
    }

    #[test]
    fn config_version_invalid() {
        assert_eq!(
            Config::new(0xb1bababa).err().unwrap(),
            Error::UnknownVersion
        );
    }

    #[test]
    fn version_negotiation() {
        let mut buf = [0; 65535];

        let mut config = Config::new(0xbabababa).unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_client_config(&mut config).unwrap();

        let (mut len, _) = pipe.client.send(&mut buf).unwrap();

        let hdr = packet::Header::from_slice(&mut buf[..len], 0).unwrap();
        len = crate::negotiate_version(&hdr.scid, &hdr.dcid, &mut buf).unwrap();

        assert_eq!(pipe.client_recv(&mut buf[..len]), Ok(len));

        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.version, PROTOCOL_VERSION);
        assert_eq!(pipe.server.version, PROTOCOL_VERSION);
    }

    #[test]
    fn verify_custom_root() {
        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config.verify_peer(true);
        config
            .load_verify_locations_from_file("examples/rootca.crt")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();

        let mut pipe = testing::Pipe::with_client_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));
    }

    // Disable this for openssl as it seems to fail for some reason. It could be
    // because of the way the get_certs API differs from bssl.
    #[cfg(not(feature = "openssl"))]
    #[test]
    fn verify_client_invalid() {
        let mut server_config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        server_config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        server_config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        server_config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        server_config.set_initial_max_data(30);
        server_config.set_initial_max_stream_data_bidi_local(15);
        server_config.set_initial_max_stream_data_bidi_remote(15);
        server_config.set_initial_max_streams_bidi(3);

        // The server shouldn't be able to verify the client's certificate due
        // to missing CA.
        server_config.verify_peer(true);

        let mut client_config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        client_config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        client_config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        client_config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        client_config.set_initial_max_data(30);
        client_config.set_initial_max_stream_data_bidi_local(15);
        client_config.set_initial_max_stream_data_bidi_remote(15);
        client_config.set_initial_max_streams_bidi(3);

        // The client is able to verify the server's certificate with the
        // appropriate CA.
        client_config
            .load_verify_locations_from_file("examples/rootca.crt")
            .unwrap();
        client_config.verify_peer(true);

        let mut pipe = testing::Pipe::with_client_and_server_config(
            &mut client_config,
            &mut server_config,
        )
        .unwrap();
        assert_eq!(pipe.handshake(), Err(Error::TlsFail));

        // Client did send a certificate.
        assert!(pipe.server.peer_cert().is_some());
    }

    #[test]
    fn verify_client_anonymous() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_streams_bidi(3);

        // Try to validate client certificate.
        config.verify_peer(true);

        let mut pipe = testing::Pipe::with_server_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client didn't send a certificate.
        assert!(pipe.server.peer_cert().is_none());
    }

    #[test]
    fn missing_initial_source_connection_id() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();

        // Reset initial_source_connection_id.
        pipe.client
            .local_transport_params
            .initial_source_connection_id = None;
        assert_eq!(pipe.client.encode_transport_params(), Ok(()));

        // Client sends initial flight.
        let (len, _) = pipe.client.send(&mut buf).unwrap();

        // Server rejects transport parameters.
        assert_eq!(
            pipe.server_recv(&mut buf[..len]),
            Err(Error::InvalidTransportParam)
        );
    }

    #[test]
    fn invalid_initial_source_connection_id() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();

        // Scramble initial_source_connection_id.
        pipe.client
            .local_transport_params
            .initial_source_connection_id = Some(b"bogus value".to_vec().into());
        assert_eq!(pipe.client.encode_transport_params(), Ok(()));

        // Client sends initial flight.
        let (len, _) = pipe.client.send(&mut buf).unwrap();

        // Server rejects transport parameters.
        assert_eq!(
            pipe.server_recv(&mut buf[..len]),
            Err(Error::InvalidTransportParam)
        );
    }

    #[test]
    fn handshake() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(
            pipe.client.application_proto(),
            pipe.server.application_proto()
        );

        assert_eq!(pipe.server.server_name(), Some("quic.tech"));
    }

    #[test]
    fn handshake_done() {
        let mut pipe = testing::Pipe::new().unwrap();

        // Disable session tickets on the server (SSL_OP_NO_TICKET) to avoid
        // triggering 1-RTT packet send with a CRYPTO frame.
        pipe.server.handshake.set_options(0x0000_4000);

        assert_eq!(pipe.handshake(), Ok(()));

        assert!(pipe.server.handshake_done_sent);
    }

    #[test]
    fn handshake_confirmation() {
        let mut pipe = testing::Pipe::new().unwrap();

        // Client sends initial flight.
        let flight = testing::emit_flight(&mut pipe.client).unwrap();
        testing::process_flight(&mut pipe.server, flight).unwrap();

        // Server sends initial flight.
        let flight = testing::emit_flight(&mut pipe.server).unwrap();

        assert!(!pipe.client.is_established());
        assert!(!pipe.client.handshake_confirmed);

        assert!(!pipe.server.is_established());
        assert!(!pipe.server.handshake_confirmed);

        testing::process_flight(&mut pipe.client, flight).unwrap();

        // Client sends Handshake packet and completes handshake.
        let flight = testing::emit_flight(&mut pipe.client).unwrap();

        assert!(pipe.client.is_established());
        assert!(!pipe.client.handshake_confirmed);

        assert!(!pipe.server.is_established());
        assert!(!pipe.server.handshake_confirmed);

        testing::process_flight(&mut pipe.server, flight).unwrap();

        // Server completes and confirms handshake, and sends HANDSHAKE_DONE.
        let flight = testing::emit_flight(&mut pipe.server).unwrap();

        assert!(pipe.client.is_established());
        assert!(!pipe.client.handshake_confirmed);

        assert!(pipe.server.is_established());
        assert!(pipe.server.handshake_confirmed);

        testing::process_flight(&mut pipe.client, flight).unwrap();

        // Client acks 1-RTT packet, and confirms handshake.
        let flight = testing::emit_flight(&mut pipe.client).unwrap();

        assert!(pipe.client.is_established());
        assert!(pipe.client.handshake_confirmed);

        assert!(pipe.server.is_established());
        assert!(pipe.server.handshake_confirmed);

        testing::process_flight(&mut pipe.server, flight).unwrap();

        assert!(pipe.client.is_established());
        assert!(pipe.client.handshake_confirmed);

        assert!(pipe.server.is_established());
        assert!(pipe.server.handshake_confirmed);
    }

    #[test]
    fn handshake_resumption() {
        #[cfg(not(feature = "openssl"))]
        const SESSION_TICKET_KEY: [u8; 48] = [0xa; 48];

        // 80-byte key(AES 256)
        // TODO: We can set the default? or query the ticket size by calling
        // the same API(SSL_CTX_set_tlsext_ticket_keys) twice to fetch the size.
        #[cfg(feature = "openssl")]
        const SESSION_TICKET_KEY: [u8; 80] = [0xa; 80];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();

        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_streams_bidi(3);
        config.set_ticket_key(&SESSION_TICKET_KEY).unwrap();

        // Perform initial handshake.
        let mut pipe = testing::Pipe::with_server_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert!(pipe.client.is_established());
        assert!(pipe.server.is_established());

        assert!(!pipe.client.is_resumed());
        assert!(!pipe.server.is_resumed());

        // Extract session,
        let session = pipe.client.session().unwrap();

        // Configure session on new connection and perform handshake.
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_streams_bidi(3);
        config.set_ticket_key(&SESSION_TICKET_KEY).unwrap();

        let mut pipe = testing::Pipe::with_server_config(&mut config).unwrap();

        assert_eq!(pipe.client.set_session(session), Ok(()));
        assert_eq!(pipe.handshake(), Ok(()));

        assert!(pipe.client.is_established());
        assert!(pipe.server.is_established());

        assert!(pipe.client.is_resumed());
        assert!(pipe.server.is_resumed());
    }

    #[test]
    fn handshake_alpn_mismatch() {
        let mut buf = [0; 65535];

        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .set_application_protos(&[b"proto3\x06proto4"])
            .unwrap();
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_client_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Err(Error::TlsFail));

        assert_eq!(pipe.client.application_proto(), b"");
        assert_eq!(pipe.server.application_proto(), b"");

        // Server should only send one packet in response to ALPN mismatch.
        let (len, _) = pipe.server.send(&mut buf).unwrap();
        assert_eq!(len, 1200);

        assert_eq!(pipe.server.send(&mut buf), Err(Error::Done));
        assert_eq!(pipe.server.sent_count, 1);
    }

    #[cfg(not(feature = "openssl"))] // 0-RTT not supported when using openssl/quictls
    #[test]
    fn handshake_0rtt() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_streams_bidi(3);
        config.enable_early_data();
        config.verify_peer(false);

        // Perform initial handshake.
        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Extract session,
        let session = pipe.client.session().unwrap();

        // Configure session on new connection.
        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.client.set_session(session), Ok(()));

        // Client sends initial flight.
        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(pipe.server_recv(&mut buf[..len]), Ok(len));

        // Client sends 0-RTT packet.
        let pkt_type = packet::Type::ZeroRTT;

        let frames = [frame::Frame::Stream {
            stream_id: 4,
            data: stream::RangeBuf::from(b"aaaaa", 0, true),
        }];

        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Ok(1200)
        );

        assert_eq!(pipe.server.undecryptable_pkts.len(), 0);

        // 0-RTT stream data is readable.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        let mut b = [0; 15];
        assert_eq!(pipe.server.stream_recv(4, &mut b), Ok((5, true)));
        assert_eq!(&b[..5], b"aaaaa");
    }

    #[cfg(not(feature = "openssl"))] // 0-RTT not supported when using openssl/quictls
    #[test]
    fn handshake_0rtt_reordered() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_streams_bidi(3);
        config.enable_early_data();
        config.verify_peer(false);

        // Perform initial handshake.
        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Extract session,
        let session = pipe.client.session().unwrap();

        // Configure session on new connection.
        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.client.set_session(session), Ok(()));

        // Client sends initial flight.
        let (len, _) = pipe.client.send(&mut buf).unwrap();
        let mut initial = buf[..len].to_vec();

        // Client sends 0-RTT packet.
        let pkt_type = packet::Type::ZeroRTT;

        let frames = [frame::Frame::Stream {
            stream_id: 4,
            data: stream::RangeBuf::from(b"aaaaa", 0, true),
        }];

        let len =
            testing::encode_pkt(&mut pipe.client, pkt_type, &frames, &mut buf)
                .unwrap();
        let mut zrtt = buf[..len].to_vec();

        // 0-RTT packet is received before the Initial one.
        assert_eq!(pipe.server_recv(&mut zrtt), Ok(zrtt.len()));

        assert_eq!(pipe.server.undecryptable_pkts.len(), 1);
        assert_eq!(pipe.server.undecryptable_pkts[0].0.len(), zrtt.len());

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        // Initial packet is also received.
        assert_eq!(pipe.server_recv(&mut initial), Ok(initial.len()));

        // 0-RTT stream data is readable.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        let mut b = [0; 15];
        assert_eq!(pipe.server.stream_recv(4, &mut b), Ok((5, true)));
        assert_eq!(&b[..5], b"aaaaa");
    }

    #[cfg(not(feature = "openssl"))] // 0-RTT not supported when using openssl/quictls
    #[test]
    fn handshake_0rtt_truncated() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_streams_bidi(3);
        config.enable_early_data();
        config.verify_peer(false);

        // Perform initial handshake.
        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Extract session,
        let session = pipe.client.session().unwrap();

        // Configure session on new connection.
        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.client.set_session(session), Ok(()));

        // Client sends initial flight.
        pipe.client.send(&mut buf).unwrap();

        // Client sends 0-RTT packet.
        let pkt_type = packet::Type::ZeroRTT;

        let frames = [frame::Frame::Stream {
            stream_id: 4,
            data: stream::RangeBuf::from(b"aaaaa", 0, true),
        }];

        let len =
            testing::encode_pkt(&mut pipe.client, pkt_type, &frames, &mut buf)
                .unwrap();

        // Simulate a truncated packet by sending one byte less.
        let mut zrtt = buf[..len - 1].to_vec();

        // 0-RTT packet is received before the Initial one.
        assert_eq!(pipe.server_recv(&mut zrtt), Err(Error::InvalidPacket));

        assert_eq!(pipe.server.undecryptable_pkts.len(), 0);

        assert!(pipe.server.is_closed());
    }

    #[test]
    fn crypto_limit() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_streams_bidi(3);
        config.enable_early_data();
        config.verify_peer(false);

        // Perform initial handshake.
        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client send a 1-byte frame that starts from the crypto stream offset
        // limit.
        let frames = [frame::Frame::Crypto {
            data: stream::RangeBuf::from(b"a", MAX_CRYPTO_STREAM_OFFSET, false),
        }];

        let pkt_type = packet::Type::Short;

        let written =
            testing::encode_pkt(&mut pipe.client, pkt_type, &frames, &mut buf)
                .unwrap();

        let active_path = pipe.server.paths.get_active().unwrap();
        let info = RecvInfo {
            to: active_path.local_addr(),
            from: active_path.peer_addr(),
        };

        assert_eq!(
            pipe.server.recv(&mut buf[..written], info),
            Err(Error::CryptoBufferExceeded)
        );

        let written = match pipe.server.send(&mut buf) {
            Ok((write, _)) => write,

            Err(_) => unreachable!(),
        };

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..written]).unwrap();
        let mut iter = frames.iter();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::ConnectionClose {
                error_code: 0x0d,
                frame_type: 0,
                reason: Vec::new(),
            })
        );
    }

    #[test]
    fn limit_handshake_data() {
        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert-big.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();

        let mut pipe = testing::Pipe::with_server_config(&mut config).unwrap();

        let flight = testing::emit_flight(&mut pipe.client).unwrap();
        let client_sent = flight.iter().fold(0, |out, p| out + p.0.len());
        testing::process_flight(&mut pipe.server, flight).unwrap();

        let flight = testing::emit_flight(&mut pipe.server).unwrap();
        let server_sent = flight.iter().fold(0, |out, p| out + p.0.len());

        assert_eq!(server_sent, client_sent * MAX_AMPLIFICATION_FACTOR);
    }

    #[test]
    fn custom_limit_handshake_data() {
        const CUSTOM_AMPLIFICATION_FACTOR: usize = 2;

        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert-big.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_max_amplification_factor(CUSTOM_AMPLIFICATION_FACTOR);

        let mut pipe = testing::Pipe::with_server_config(&mut config).unwrap();

        let flight = testing::emit_flight(&mut pipe.client).unwrap();
        let client_sent = flight.iter().fold(0, |out, p| out + p.0.len());
        testing::process_flight(&mut pipe.server, flight).unwrap();

        let flight = testing::emit_flight(&mut pipe.server).unwrap();
        let server_sent = flight.iter().fold(0, |out, p| out + p.0.len());

        assert_eq!(server_sent, client_sent * CUSTOM_AMPLIFICATION_FACTOR);
    }

    #[test]
    fn stream() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.stream_send(4, b"hello, world", true), Ok(12));
        assert_eq!(pipe.advance(), Ok(()));

        assert!(!pipe.server.stream_finished(4));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        let mut b = [0; 15];
        assert_eq!(pipe.server.stream_recv(4, &mut b), Ok((12, true)));
        assert_eq!(&b[..12], b"hello, world");

        assert!(pipe.server.stream_finished(4));
    }

    #[cfg(not(feature = "openssl"))] // 0-RTT not supported when using openssl/quictls
    #[test]
    fn zero_rtt() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_streams_bidi(3);
        config.enable_early_data();
        config.verify_peer(false);

        // Perform initial handshake.
        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Extract session,
        let session = pipe.client.session().unwrap();

        // Configure session on new connection.
        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.client.set_session(session), Ok(()));

        // Client sends initial flight.
        let (len, _) = pipe.client.send(&mut buf).unwrap();
        let mut initial = buf[..len].to_vec();

        assert!(pipe.client.is_in_early_data());

        // Client sends 0-RTT data.
        assert_eq!(pipe.client.stream_send(4, b"hello, world", true), Ok(12));

        let (len, _) = pipe.client.send(&mut buf).unwrap();
        let mut zrtt = buf[..len].to_vec();

        // Server receives packets.
        assert_eq!(pipe.server_recv(&mut initial), Ok(initial.len()));
        assert!(pipe.server.is_in_early_data());

        assert_eq!(pipe.server_recv(&mut zrtt), Ok(zrtt.len()));

        // 0-RTT stream data is readable.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        let mut b = [0; 15];
        assert_eq!(pipe.server.stream_recv(4, &mut b), Ok((12, true)));
        assert_eq!(&b[..12], b"hello, world");
    }

    #[test]
    fn stream_send_on_32bit_arch() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(2_u64.pow(32) + 5);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(0);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // In 32bit arch, send_capacity() should be min(2^32+5, cwnd),
        // not min(5, cwnd)
        assert_eq!(pipe.client.stream_send(4, b"hello, world", true), Ok(12));

        assert_eq!(pipe.advance(), Ok(()));

        assert!(!pipe.server.stream_finished(4));
    }

    #[test]
    fn empty_stream_frame() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [frame::Frame::Stream {
            stream_id: 4,
            data: stream::RangeBuf::from(b"aaaaa", 0, false),
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf), Ok(39));

        let mut readable = pipe.server.readable();
        assert_eq!(readable.next(), Some(4));

        assert_eq!(pipe.server.stream_recv(4, &mut buf), Ok((5, false)));

        let frames = [frame::Frame::Stream {
            stream_id: 4,
            data: stream::RangeBuf::from(b"", 5, true),
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf), Ok(39));

        let mut readable = pipe.server.readable();
        assert_eq!(readable.next(), Some(4));

        assert_eq!(pipe.server.stream_recv(4, &mut buf), Ok((0, true)));

        let frames = [frame::Frame::Stream {
            stream_id: 4,
            data: stream::RangeBuf::from(b"", 15, true),
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::FinalSize)
        );
    }

    #[test]
    fn update_key_request() {
        let mut b = [0; 15];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));
        assert_eq!(pipe.advance(), Ok(()));

        // Client sends message with key update request.
        assert_eq!(pipe.client_update_key(), Ok(()));
        assert_eq!(pipe.client.stream_send(4, b"hello", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Ensure server updates key and it correctly decrypts the message.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);
        assert_eq!(pipe.server.stream_recv(4, &mut b), Ok((5, false)));
        assert_eq!(&b[..5], b"hello");

        // Ensure ACK for key update.
        assert!(
            pipe.server.pkt_num_spaces[packet::Epoch::Application]
                .key_update
                .as_ref()
                .unwrap()
                .update_acked
        );

        // Server sends message with the new key.
        assert_eq!(pipe.server.stream_send(4, b"world", true), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Ensure update key is completed and client can decrypt packet.
        let mut r = pipe.client.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);
        assert_eq!(pipe.client.stream_recv(4, &mut b), Ok((5, true)));
        assert_eq!(&b[..5], b"world");
    }

    #[test]
    fn update_key_request_twice_error() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));
        assert_eq!(pipe.advance(), Ok(()));

        let frames = [frame::Frame::Stream {
            stream_id: 4,
            data: stream::RangeBuf::from(b"hello", 0, false),
        }];

        // Client sends stream frame with key update request.
        assert_eq!(pipe.client_update_key(), Ok(()));
        let written = testing::encode_pkt(
            &mut pipe.client,
            packet::Type::Short,
            &frames,
            &mut buf,
        )
        .unwrap();

        // Server correctly decode with new key.
        assert_eq!(pipe.server_recv(&mut buf[..written]), Ok(written));

        // Client sends stream frame with another key update request before server
        // ACK.
        assert_eq!(pipe.client_update_key(), Ok(()));
        let written = testing::encode_pkt(
            &mut pipe.client,
            packet::Type::Short,
            &frames,
            &mut buf,
        )
        .unwrap();

        // Check server correctly closes the connection with a key update error
        // for the peer.
        assert_eq!(pipe.server_recv(&mut buf[..written]), Err(Error::KeyUpdate));
    }

    #[test]
    /// Tests that receiving a MAX_STREAM_DATA frame for a receive-only
    /// unidirectional stream is forbidden.
    fn max_stream_data_receive_uni() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client opens unidirectional stream.
        assert_eq!(pipe.client.stream_send(2, b"hello", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Client sends MAX_STREAM_DATA on local unidirectional stream.
        let frames = [frame::Frame::MaxStreamData {
            stream_id: 2,
            max: 1024,
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::InvalidStreamState(2)),
        );
    }

    #[test]
    fn empty_payload() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Send a packet with no frames.
        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &[], &mut buf),
            Err(Error::InvalidPacket)
        );
    }

    #[test]
    fn min_payload() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();

        // Send a non-ack-eliciting packet.
        let frames = [frame::Frame::Padding { len: 4 }];

        let pkt_type = packet::Type::Initial;
        let written =
            testing::encode_pkt(&mut pipe.client, pkt_type, &frames, &mut buf)
                .unwrap();
        assert_eq!(pipe.server_recv(&mut buf[..written]), Ok(written));

        let initial_path = pipe
            .server
            .paths
            .get_active()
            .expect("initial path not found");

        assert_eq!(initial_path.max_send_bytes, 195);

        // Force server to send a single PING frame.
        pipe.server
            .paths
            .get_active_mut()
            .expect("no active path")
            .recovery
            .inc_loss_probes(packet::Epoch::Initial);

        let initial_path = pipe
            .server
            .paths
            .get_active_mut()
            .expect("initial path not found");

        // Artificially limit the amount of bytes the server can send.
        initial_path.max_send_bytes = 60;

        assert_eq!(pipe.server.send(&mut buf), Err(Error::Done));
    }

    #[test]
    fn flow_control_limit() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [
            frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"aaaaaaaaaaaaaaa", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 4,
                data: stream::RangeBuf::from(b"aaaaaaaaaaaaaaa", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 8,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
        ];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::FlowControl),
        );
    }

    #[test]
    fn flow_control_limit_dup() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [
            // One byte less than stream limit.
            frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"aaaaaaaaaaaaaa", 0, false),
            },
            // Same stream, but one byte more.
            frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"aaaaaaaaaaaaaaa", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 8,
                data: stream::RangeBuf::from(b"aaaaaaaaaaaaaaa", 0, false),
            },
        ];

        let pkt_type = packet::Type::Short;
        assert!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf).is_ok());
    }

    #[test]
    fn flow_control_update() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [
            frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"aaaaaaaaaaaaaaa", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 4,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
        ];

        let pkt_type = packet::Type::Short;

        assert!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf).is_ok());

        pipe.server.stream_recv(0, &mut buf).unwrap();
        pipe.server.stream_recv(4, &mut buf).unwrap();

        let frames = [frame::Frame::Stream {
            stream_id: 4,
            data: stream::RangeBuf::from(b"a", 1, false),
        }];

        let len = pipe
            .send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();

        assert!(len > 0);

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
        let mut iter = frames.iter();

        // Ignore ACK.
        iter.next().unwrap();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::MaxStreamData {
                stream_id: 0,
                max: 30
            })
        );
        assert_eq!(iter.next(), Some(&frame::Frame::MaxData { max: 61 }));
    }

    #[test]
    /// Tests that flow control is properly updated even when a stream is shut
    /// down.
    fn flow_control_drain() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client opens a stream and sends some data.
        assert_eq!(pipe.client.stream_send(4, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Server receives data, without reading it.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        // In the meantime, client sends more data.
        assert_eq!(pipe.client.stream_send(4, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.client.stream_send(4, b"aaaaa", true), Ok(5));

        assert_eq!(pipe.client.stream_send(8, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.client.stream_send(8, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.client.stream_send(8, b"aaaaa", true), Ok(5));

        // Server shuts down one stream.
        assert_eq!(pipe.server.stream_shutdown(4, Shutdown::Read, 42), Ok(()));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        // Flush connection.
        assert_eq!(pipe.advance(), Ok(()));
    }

    #[test]
    fn stream_flow_control_limit_bidi() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [frame::Frame::Stream {
            stream_id: 4,
            data: stream::RangeBuf::from(b"aaaaaaaaaaaaaaaa", 0, true),
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::FlowControl),
        );
    }

    #[test]
    fn stream_flow_control_limit_uni() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [frame::Frame::Stream {
            stream_id: 2,
            data: stream::RangeBuf::from(b"aaaaaaaaaaa", 0, true),
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::FlowControl),
        );
    }

    #[test]
    fn stream_flow_control_update() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [frame::Frame::Stream {
            stream_id: 4,
            data: stream::RangeBuf::from(b"aaaaaaaaa", 0, false),
        }];

        let pkt_type = packet::Type::Short;

        assert!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf).is_ok());

        pipe.server.stream_recv(4, &mut buf).unwrap();

        let frames = [frame::Frame::Stream {
            stream_id: 4,
            data: stream::RangeBuf::from(b"a", 9, false),
        }];

        let len = pipe
            .send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();

        assert!(len > 0);

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
        let mut iter = frames.iter();

        // Ignore ACK.
        iter.next().unwrap();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::MaxStreamData {
                stream_id: 4,
                max: 24,
            })
        );
    }

    #[test]
    fn stream_left_bidi() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(3, pipe.client.peer_streams_left_bidi());
        assert_eq!(3, pipe.server.peer_streams_left_bidi());

        pipe.server.stream_send(1, b"a", false).ok();
        assert_eq!(2, pipe.server.peer_streams_left_bidi());
        pipe.server.stream_send(5, b"a", false).ok();
        assert_eq!(1, pipe.server.peer_streams_left_bidi());

        pipe.server.stream_send(9, b"a", false).ok();
        assert_eq!(0, pipe.server.peer_streams_left_bidi());

        let frames = [frame::Frame::MaxStreamsBidi { max: MAX_STREAM_ID }];

        let pkt_type = packet::Type::Short;
        assert!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf).is_ok());

        assert_eq!(MAX_STREAM_ID - 3, pipe.server.peer_streams_left_bidi());
    }

    #[test]
    fn stream_left_uni() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(3, pipe.client.peer_streams_left_uni());
        assert_eq!(3, pipe.server.peer_streams_left_uni());

        pipe.server.stream_send(3, b"a", false).ok();
        assert_eq!(2, pipe.server.peer_streams_left_uni());
        pipe.server.stream_send(7, b"a", false).ok();
        assert_eq!(1, pipe.server.peer_streams_left_uni());

        pipe.server.stream_send(11, b"a", false).ok();
        assert_eq!(0, pipe.server.peer_streams_left_uni());

        let frames = [frame::Frame::MaxStreamsUni { max: MAX_STREAM_ID }];

        let pkt_type = packet::Type::Short;
        assert!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf).is_ok());

        assert_eq!(MAX_STREAM_ID - 3, pipe.server.peer_streams_left_uni());
    }

    #[test]
    fn stream_limit_bidi() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [
            frame::Frame::Stream {
                stream_id: 4,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 8,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 12,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 16,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 20,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 24,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 28,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
        ];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::StreamLimit),
        );
    }

    #[test]
    fn stream_limit_max_bidi() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [frame::Frame::MaxStreamsBidi { max: MAX_STREAM_ID }];

        let pkt_type = packet::Type::Short;
        assert!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf).is_ok());

        let frames = [frame::Frame::MaxStreamsBidi {
            max: MAX_STREAM_ID + 1,
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::InvalidFrame),
        );
    }

    #[test]
    fn stream_limit_uni() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [
            frame::Frame::Stream {
                stream_id: 2,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 6,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 10,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 14,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 18,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 22,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 26,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
        ];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::StreamLimit),
        );
    }

    #[test]
    fn stream_limit_max_uni() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [frame::Frame::MaxStreamsUni { max: MAX_STREAM_ID }];

        let pkt_type = packet::Type::Short;
        assert!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf).is_ok());

        let frames = [frame::Frame::MaxStreamsUni {
            max: MAX_STREAM_ID + 1,
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::InvalidFrame),
        );
    }

    #[test]
    fn stream_left_reset_bidi() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(3, pipe.client.peer_streams_left_bidi());
        assert_eq!(3, pipe.server.peer_streams_left_bidi());

        pipe.client.stream_send(0, b"a", false).ok();
        assert_eq!(2, pipe.client.peer_streams_left_bidi());
        pipe.client.stream_send(4, b"a", false).ok();
        assert_eq!(1, pipe.client.peer_streams_left_bidi());
        pipe.client.stream_send(8, b"a", false).ok();
        assert_eq!(0, pipe.client.peer_streams_left_bidi());

        // Client resets the stream.
        pipe.client
            .stream_shutdown(0, Shutdown::Write, 1001)
            .unwrap();
        pipe.advance().unwrap();

        assert_eq!(0, pipe.client.peer_streams_left_bidi());
        let mut r = pipe.server.readable();
        assert_eq!(Some(0), r.next());
        assert_eq!(Some(4), r.next());
        assert_eq!(Some(8), r.next());
        assert_eq!(None, r.next());

        assert_eq!(
            pipe.server.stream_recv(0, &mut buf),
            Err(Error::StreamReset(1001))
        );

        let mut r = pipe.server.readable();
        assert_eq!(Some(4), r.next());
        assert_eq!(Some(8), r.next());
        assert_eq!(None, r.next());

        // Server resets the stream in reaction.
        pipe.server
            .stream_shutdown(0, Shutdown::Write, 1001)
            .unwrap();
        pipe.advance().unwrap();

        assert_eq!(1, pipe.client.peer_streams_left_bidi());

        // Repeat for the other 2 streams
        pipe.client
            .stream_shutdown(4, Shutdown::Write, 1001)
            .unwrap();
        pipe.client
            .stream_shutdown(8, Shutdown::Write, 1001)
            .unwrap();
        pipe.advance().unwrap();

        let mut r = pipe.server.readable();
        assert_eq!(Some(4), r.next());
        assert_eq!(Some(8), r.next());
        assert_eq!(None, r.next());

        assert_eq!(
            pipe.server.stream_recv(4, &mut buf),
            Err(Error::StreamReset(1001))
        );

        assert_eq!(
            pipe.server.stream_recv(8, &mut buf),
            Err(Error::StreamReset(1001))
        );

        let mut r = pipe.server.readable();
        assert_eq!(None, r.next());

        pipe.server
            .stream_shutdown(4, Shutdown::Write, 1001)
            .unwrap();
        pipe.server
            .stream_shutdown(8, Shutdown::Write, 1001)
            .unwrap();
        pipe.advance().unwrap();

        assert_eq!(3, pipe.client.peer_streams_left_bidi());
    }

    #[test]
    fn stream_reset_counts() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        pipe.client.stream_send(0, b"a", false).ok();
        pipe.client.stream_send(2, b"a", false).ok();
        pipe.client.stream_send(4, b"a", false).ok();
        pipe.client.stream_send(8, b"a", false).ok();
        pipe.advance().unwrap();

        let stats = pipe.client.stats();
        assert_eq!(stats.reset_stream_count_local, 0);

        // Client resets the stream.
        pipe.client
            .stream_shutdown(0, Shutdown::Write, 1001)
            .unwrap();
        pipe.advance().unwrap();

        let stats = pipe.client.stats();
        assert_eq!(stats.reset_stream_count_local, 1);
        assert_eq!(stats.reset_stream_count_remote, 0);
        let stats = pipe.server.stats();
        assert_eq!(stats.reset_stream_count_local, 0);
        assert_eq!(stats.reset_stream_count_remote, 1);

        // Server resets the stream in reaction.
        pipe.server
            .stream_shutdown(0, Shutdown::Write, 1001)
            .unwrap();
        pipe.advance().unwrap();

        let stats = pipe.client.stats();
        assert_eq!(stats.reset_stream_count_local, 1);
        assert_eq!(stats.reset_stream_count_remote, 1);
        let stats = pipe.server.stats();
        assert_eq!(stats.reset_stream_count_local, 1);
        assert_eq!(stats.reset_stream_count_remote, 1);

        // Repeat for the other streams
        pipe.client
            .stream_shutdown(2, Shutdown::Write, 1001)
            .unwrap();
        pipe.client
            .stream_shutdown(4, Shutdown::Write, 1001)
            .unwrap();
        pipe.client
            .stream_shutdown(8, Shutdown::Write, 1001)
            .unwrap();
        pipe.advance().unwrap();

        pipe.server
            .stream_shutdown(4, Shutdown::Write, 1001)
            .unwrap();
        pipe.server
            .stream_shutdown(8, Shutdown::Write, 1001)
            .unwrap();
        pipe.advance().unwrap();

        let stats = pipe.client.stats();
        assert_eq!(stats.reset_stream_count_local, 4);
        assert_eq!(stats.reset_stream_count_remote, 3);
        let stats = pipe.server.stats();
        assert_eq!(stats.reset_stream_count_local, 3);
        assert_eq!(stats.reset_stream_count_remote, 4);
    }

    #[test]
    fn stream_stop_counts() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        pipe.client.stream_send(0, b"a", false).ok();
        pipe.client.stream_send(2, b"a", false).ok();
        pipe.client.stream_send(4, b"a", false).ok();
        pipe.client.stream_send(8, b"a", false).ok();
        pipe.advance().unwrap();

        let stats = pipe.client.stats();
        assert_eq!(stats.reset_stream_count_local, 0);

        // Server stops the stream and client automatically resets.
        pipe.server
            .stream_shutdown(0, Shutdown::Read, 1001)
            .unwrap();
        pipe.advance().unwrap();

        let stats = pipe.client.stats();
        assert_eq!(stats.stopped_stream_count_local, 0);
        assert_eq!(stats.stopped_stream_count_remote, 1);
        assert_eq!(stats.reset_stream_count_local, 1);
        assert_eq!(stats.reset_stream_count_remote, 0);

        let stats = pipe.server.stats();
        assert_eq!(stats.stopped_stream_count_local, 1);
        assert_eq!(stats.stopped_stream_count_remote, 0);
        assert_eq!(stats.reset_stream_count_local, 0);
        assert_eq!(stats.reset_stream_count_remote, 1);

        // Repeat for the other streams
        pipe.server
            .stream_shutdown(2, Shutdown::Read, 1001)
            .unwrap();
        pipe.server
            .stream_shutdown(4, Shutdown::Read, 1001)
            .unwrap();
        pipe.server
            .stream_shutdown(8, Shutdown::Read, 1001)
            .unwrap();
        pipe.advance().unwrap();

        let stats = pipe.client.stats();
        assert_eq!(stats.stopped_stream_count_local, 0);
        assert_eq!(stats.stopped_stream_count_remote, 4);
        assert_eq!(stats.reset_stream_count_local, 4);
        assert_eq!(stats.reset_stream_count_remote, 0);

        let stats = pipe.server.stats();
        assert_eq!(stats.stopped_stream_count_local, 4);
        assert_eq!(stats.stopped_stream_count_remote, 0);
        assert_eq!(stats.reset_stream_count_local, 0);
        assert_eq!(stats.reset_stream_count_remote, 4);
    }

    #[test]
    fn streams_blocked_max_bidi() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [frame::Frame::StreamsBlockedBidi {
            limit: MAX_STREAM_ID,
        }];

        let pkt_type = packet::Type::Short;
        assert!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf).is_ok());

        let frames = [frame::Frame::StreamsBlockedBidi {
            limit: MAX_STREAM_ID + 1,
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::InvalidFrame),
        );
    }

    #[test]
    fn streams_blocked_max_uni() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [frame::Frame::StreamsBlockedUni {
            limit: MAX_STREAM_ID,
        }];

        let pkt_type = packet::Type::Short;
        assert!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf).is_ok());

        let frames = [frame::Frame::StreamsBlockedUni {
            limit: MAX_STREAM_ID + 1,
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::InvalidFrame),
        );
    }

    #[test]
    fn stream_data_overlap() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [
            frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"aaaaa", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"bbbbb", 3, false),
            },
            frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"ccccc", 6, false),
            },
        ];

        let pkt_type = packet::Type::Short;
        assert!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf).is_ok());

        let mut b = [0; 15];
        assert_eq!(pipe.server.stream_recv(0, &mut b), Ok((11, false)));
        assert_eq!(&b[..11], b"aaaaabbbccc");
    }

    #[test]
    fn stream_data_overlap_with_reordering() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [
            frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"aaaaa", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"ccccc", 6, false),
            },
            frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"bbbbb", 3, false),
            },
        ];

        let pkt_type = packet::Type::Short;
        assert!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf).is_ok());

        let mut b = [0; 15];
        assert_eq!(pipe.server.stream_recv(0, &mut b), Ok((11, false)));
        assert_eq!(&b[..11], b"aaaaabccccc");
    }

    #[test]
    /// Tests that receiving a valid RESET_STREAM frame when all data has
    /// already been read, notifies the application.
    fn reset_stream_data_recvd() {
        let mut b = [0; 15];
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends some data.
        assert_eq!(pipe.client.stream_send(0, b"hello", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Server gets data and sends data back, closing stream.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.server.stream_recv(0, &mut b), Ok((5, false)));
        assert!(!pipe.server.stream_finished(0));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        assert_eq!(pipe.server.stream_send(0, b"", true), Ok(0));
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.client.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.client.stream_recv(0, &mut b), Ok((0, true)));
        assert!(pipe.client.stream_finished(0));

        // Client sends RESET_STREAM, closing stream.
        let frames = [frame::Frame::ResetStream {
            stream_id: 0,
            error_code: 42,
            final_size: 5,
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf), Ok(39));

        // Server is notified of stream readability, due to reset.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(
            pipe.server.stream_recv(0, &mut b),
            Err(Error::StreamReset(42))
        );

        assert!(pipe.server.stream_finished(0));

        // Sending RESET_STREAM again shouldn't make stream readable again.
        pipe.send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);
    }

    #[test]
    /// Tests that receiving a valid RESET_STREAM frame when all data has _not_
    /// been read, discards all buffered data and notifies the application.
    fn reset_stream_data_not_recvd() {
        let mut b = [0; 15];
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends some data.
        assert_eq!(pipe.client.stream_send(0, b"h", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        // Server gets data and sends data back, closing stream.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.server.stream_recv(0, &mut b), Ok((1, false)));
        assert!(!pipe.server.stream_finished(0));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        assert_eq!(pipe.server.stream_send(0, b"", true), Ok(0));
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.client.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.client.stream_recv(0, &mut b), Ok((0, true)));
        assert!(pipe.client.stream_finished(0));

        // Client sends RESET_STREAM, closing stream.
        let frames = [frame::Frame::ResetStream {
            stream_id: 0,
            error_code: 42,
            final_size: 5,
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf), Ok(39));

        // Server is notified of stream readability, due to reset.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(
            pipe.server.stream_recv(0, &mut b),
            Err(Error::StreamReset(42))
        );

        assert!(pipe.server.stream_finished(0));

        // Sending RESET_STREAM again shouldn't make stream readable again.
        assert_eq!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf), Ok(39));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);
    }

    #[test]
    /// Tests that RESET_STREAM frames exceeding the connection-level flow
    /// control limit cause an error.
    fn reset_stream_flow_control() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [
            frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"aaaaaaaaaaaaaaa", 0, false),
            },
            frame::Frame::Stream {
                stream_id: 4,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::ResetStream {
                stream_id: 4,
                error_code: 0,
                final_size: 15,
            },
            frame::Frame::Stream {
                stream_id: 8,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
        ];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::FlowControl),
        );
    }

    #[test]
    /// Tests that RESET_STREAM frames exceeding the stream-level flow control
    /// limit cause an error.
    fn reset_stream_flow_control_stream() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [
            frame::Frame::Stream {
                stream_id: 4,
                data: stream::RangeBuf::from(b"a", 0, false),
            },
            frame::Frame::ResetStream {
                stream_id: 4,
                error_code: 0,
                final_size: 16, // Past stream's flow control limit.
            },
        ];

        let pkt_type = packet::Type::Short;
        assert_eq!(
            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf),
            Err(Error::FlowControl),
        );
    }

    #[test]
    fn path_challenge() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [frame::Frame::PathChallenge { data: [0xba; 8] }];

        let pkt_type = packet::Type::Short;

        let len = pipe
            .send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();

        assert!(len > 0);

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
        let mut iter = frames.iter();

        // Ignore ACK.
        iter.next().unwrap();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::PathResponse { data: [0xba; 8] })
        );
    }

    #[cfg(not(feature = "openssl"))] // 0-RTT not supported when using openssl/quictls
    #[test]
    /// Simulates reception of an early 1-RTT packet on the server, by
    /// delaying the client's Handshake packet that completes the handshake.
    fn early_1rtt_packet() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();

        // Client sends initial flight
        let flight = testing::emit_flight(&mut pipe.client).unwrap();
        testing::process_flight(&mut pipe.server, flight).unwrap();

        // Server sends initial flight.
        let flight = testing::emit_flight(&mut pipe.server).unwrap();
        testing::process_flight(&mut pipe.client, flight).unwrap();

        // Client sends Handshake packet.
        let flight = testing::emit_flight(&mut pipe.client).unwrap();

        // Emulate handshake packet delay by not making server process client
        // packet.
        let delayed = flight;

        testing::emit_flight(&mut pipe.server).ok();

        assert!(pipe.client.is_established());

        // Send 1-RTT packet #0.
        let frames = [frame::Frame::Stream {
            stream_id: 0,
            data: stream::RangeBuf::from(b"hello, world", 0, true),
        }];

        let pkt_type = packet::Type::Short;
        let written =
            testing::encode_pkt(&mut pipe.client, pkt_type, &frames, &mut buf)
                .unwrap();

        assert_eq!(pipe.server_recv(&mut buf[..written]), Ok(written));

        // Send 1-RTT packet #1.
        let frames = [frame::Frame::Stream {
            stream_id: 4,
            data: stream::RangeBuf::from(b"hello, world", 0, true),
        }];

        let written =
            testing::encode_pkt(&mut pipe.client, pkt_type, &frames, &mut buf)
                .unwrap();

        assert_eq!(pipe.server_recv(&mut buf[..written]), Ok(written));

        assert!(!pipe.server.is_established());

        // Client sent 1-RTT packets 0 and 1, but server hasn't received them.
        //
        // Note that `largest_rx_pkt_num` is initialized to 0, so we need to
        // send another 1-RTT packet to make this check meaningful.
        assert_eq!(
            pipe.server.pkt_num_spaces[packet::Epoch::Application]
                .largest_rx_pkt_num,
            0
        );

        // Process delayed packet.
        testing::process_flight(&mut pipe.server, delayed).unwrap();

        assert!(pipe.server.is_established());

        assert_eq!(
            pipe.server.pkt_num_spaces[packet::Epoch::Application]
                .largest_rx_pkt_num,
            0
        );
    }

    #[test]
    fn stop_sending() {
        let mut b = [0; 15];

        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends some data, and closes stream.
        assert_eq!(pipe.client.stream_send(0, b"hello", true), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Server gets data.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.server.stream_recv(0, &mut b), Ok((5, true)));
        assert!(pipe.server.stream_finished(0));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        // Server sends data, until blocked.
        let mut r = pipe.server.writable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        while pipe.server.stream_send(0, b"world", false) != Err(Error::Done) {
            assert_eq!(pipe.advance(), Ok(()));
        }

        let mut r = pipe.server.writable();
        assert_eq!(r.next(), None);

        // Client sends STOP_SENDING.
        let frames = [frame::Frame::StopSending {
            stream_id: 0,
            error_code: 42,
        }];

        let pkt_type = packet::Type::Short;
        let len = pipe
            .send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();

        // Server sent a RESET_STREAM frame in response.
        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();

        let mut iter = frames.iter();

        // Skip ACK frame.
        iter.next();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::ResetStream {
                stream_id: 0,
                error_code: 42,
                final_size: 15,
            })
        );

        // Stream is writable, but writing returns an error.
        let mut r = pipe.server.writable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(
            pipe.server.stream_send(0, b"world", true),
            Err(Error::StreamStopped(42)),
        );

        assert_eq!(pipe.server.streams.len(), 1);

        // Client acks RESET_STREAM frame.
        let mut ranges = ranges::RangeSet::default();
        ranges.insert(0..6);

        let frames = [frame::Frame::ACK {
            ack_delay: 15,
            ranges,
            ecn_counts: None,
        }];

        assert_eq!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf), Ok(0));

        // Stream is collected on the server after RESET_STREAM is acked.
        assert_eq!(pipe.server.streams.len(), 0);

        // Sending STOP_SENDING again shouldn't trigger RESET_STREAM again.
        let frames = [frame::Frame::StopSending {
            stream_id: 0,
            error_code: 42,
        }];

        let len = pipe
            .send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();

        assert_eq!(frames.len(), 1);

        match frames.first() {
            Some(frame::Frame::ACK { .. }) => (),

            f => panic!("expected ACK frame, got {:?}", f),
        };

        let mut r = pipe.server.writable();
        assert_eq!(r.next(), None);
    }

    #[test]
    fn stop_sending_fin() {
        let mut b = [0; 15];

        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends some data, and closes stream.
        assert_eq!(pipe.client.stream_send(4, b"hello", true), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Server gets data.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.server.stream_recv(4, &mut b), Ok((5, true)));
        assert!(pipe.server.stream_finished(4));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        // Server sends data...
        let mut r = pipe.server.writable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.server.stream_send(4, b"world", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // ...and buffers more, and closes stream.
        assert_eq!(pipe.server.stream_send(4, b"world", true), Ok(5));

        // Client sends STOP_SENDING before server flushes stream.
        let frames = [frame::Frame::StopSending {
            stream_id: 4,
            error_code: 42,
        }];

        let pkt_type = packet::Type::Short;
        let len = pipe
            .send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();

        // Server sent a RESET_STREAM frame in response.
        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();

        let mut iter = frames.iter();

        // Skip ACK frame.
        iter.next();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::ResetStream {
                stream_id: 4,
                error_code: 42,
                final_size: 5,
            })
        );

        // No more frames are sent by the server.
        assert_eq!(iter.next(), None);
    }

    #[test]
    /// Tests that resetting a stream restores flow control for unsent data.
    fn stop_sending_unsent_tx_cap() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(15);
        config.set_initial_max_stream_data_bidi_local(30);
        config.set_initial_max_stream_data_bidi_remote(30);
        config.set_initial_max_stream_data_uni(30);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(0);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends some data.
        assert_eq!(pipe.client.stream_send(4, b"hello", true), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        let mut b = [0; 15];
        assert_eq!(pipe.server.stream_recv(4, &mut b), Ok((5, true)));

        // Server sends some data.
        assert_eq!(pipe.server.stream_send(4, b"hello", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Server buffers some data, until send capacity limit reached.
        assert_eq!(pipe.server.stream_send(4, b"hello", false), Ok(5));
        assert_eq!(pipe.server.stream_send(4, b"hello", false), Ok(5));
        assert_eq!(
            pipe.server.stream_send(4, b"hello", false),
            Err(Error::Done)
        );

        // Client sends STOP_SENDING.
        let frames = [frame::Frame::StopSending {
            stream_id: 4,
            error_code: 42,
        }];

        let pkt_type = packet::Type::Short;
        pipe.send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();

        // Server can now send more data (on a different stream).
        assert_eq!(pipe.client.stream_send(8, b"hello", true), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.server.stream_send(8, b"hello", false), Ok(5));
        assert_eq!(pipe.server.stream_send(8, b"hello", false), Ok(5));
        assert_eq!(
            pipe.server.stream_send(8, b"hello", false),
            Err(Error::Done)
        );
        assert_eq!(pipe.advance(), Ok(()));
    }

    #[test]
    fn stream_shutdown_read() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends some data.
        assert_eq!(pipe.client.stream_send(4, b"hello, world", false), Ok(12));
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.client.streams.len(), 1);
        assert_eq!(pipe.server.streams.len(), 1);

        // Server shuts down stream.
        assert_eq!(pipe.server.stream_shutdown(4, Shutdown::Read, 42), Ok(()));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        let (len, _) = pipe.server.send(&mut buf).unwrap();

        let mut dummy = buf[..len].to_vec();

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut dummy[..len]).unwrap();
        let mut iter = frames.iter();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::StopSending {
                stream_id: 4,
                error_code: 42,
            })
        );

        assert_eq!(pipe.client_recv(&mut buf[..len]), Ok(len));

        assert_eq!(pipe.advance(), Ok(()));

        // Sending more data is forbidden.
        let mut r = pipe.client.writable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        assert_eq!(
            pipe.client.stream_send(4, b"bye", false),
            Err(Error::StreamStopped(42))
        );

        // Server sends some data, without reading the incoming data, and closes
        // the stream.
        assert_eq!(pipe.server.stream_send(4, b"hello, world", true), Ok(12));
        assert_eq!(pipe.advance(), Ok(()));

        // Client reads the data.
        let mut r = pipe.client.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.client.stream_recv(4, &mut buf), Ok((12, true)));

        // Stream is collected on both sides.
        assert_eq!(pipe.client.streams.len(), 0);
        assert_eq!(pipe.server.streams.len(), 0);

        assert_eq!(
            pipe.server.stream_shutdown(4, Shutdown::Read, 0),
            Err(Error::Done)
        );
    }

    #[test]
    fn stream_shutdown_read_after_fin() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends some data.
        assert_eq!(pipe.client.stream_send(4, b"hello, world", true), Ok(12));
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.client.streams.len(), 1);
        assert_eq!(pipe.server.streams.len(), 1);

        // Server shuts down stream.
        assert_eq!(pipe.server.stream_shutdown(4, Shutdown::Read, 42), Ok(()));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        // Server has nothing to send.
        assert_eq!(pipe.server.send(&mut buf), Err(Error::Done));

        assert_eq!(pipe.advance(), Ok(()));

        // Server sends some data, without reading the incoming data, and closes
        // the stream.
        assert_eq!(pipe.server.stream_send(4, b"hello, world", true), Ok(12));
        assert_eq!(pipe.advance(), Ok(()));

        // Client reads the data.
        let mut r = pipe.client.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.client.stream_recv(4, &mut buf), Ok((12, true)));

        // Stream is collected on both sides.
        assert_eq!(pipe.client.streams.len(), 0);
        assert_eq!(pipe.server.streams.len(), 0);

        assert_eq!(
            pipe.server.stream_shutdown(4, Shutdown::Read, 0),
            Err(Error::Done)
        );
    }

    #[test]
    fn stream_shutdown_read_update_max_data() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(10000);
        config.set_initial_max_stream_data_bidi_remote(10000);
        config.set_initial_max_streams_bidi(10);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.stream_send(0, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.server.stream_recv(0, &mut buf), Ok((1, false)));
        assert_eq!(pipe.server.stream_shutdown(0, Shutdown::Read, 123), Ok(()));

        assert_eq!(pipe.server.rx_data, 1);
        assert_eq!(pipe.client.tx_data, 1);
        assert_eq!(pipe.client.max_tx_data, 30);

        assert_eq!(
            pipe.client
                .stream_send(0, &buf[..pipe.client.tx_cap], false),
            Ok(29)
        );
        assert_eq!(pipe.advance(), Ok(()));

        assert!(!pipe.server.stream_readable(0)); // nothing can be consumed

        // The client has increased its tx_data, and server has received it, so
        // it increases flow control accordingly.
        assert_eq!(pipe.client.tx_data, 30);
        assert_eq!(pipe.server.rx_data, 30);
        assert_eq!(pipe.client.tx_cap, 45);
    }

    #[test]
    fn stream_shutdown_uni() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Exchange some data on uni streams.
        assert_eq!(pipe.client.stream_send(2, b"hello, world", false), Ok(10));
        assert_eq!(pipe.server.stream_send(3, b"hello, world", false), Ok(10));
        assert_eq!(pipe.advance(), Ok(()));

        // Test local and remote shutdown.
        assert_eq!(pipe.client.stream_shutdown(2, Shutdown::Write, 42), Ok(()));
        assert_eq!(
            pipe.client.stream_shutdown(2, Shutdown::Read, 42),
            Err(Error::InvalidStreamState(2))
        );

        assert_eq!(
            pipe.client.stream_shutdown(3, Shutdown::Write, 42),
            Err(Error::InvalidStreamState(3))
        );
        assert_eq!(pipe.client.stream_shutdown(3, Shutdown::Read, 42), Ok(()));
    }

    #[test]
    fn stream_shutdown_write() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends some data.
        assert_eq!(pipe.client.stream_send(4, b"hello, world", false), Ok(12));
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        let mut r = pipe.server.writable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.client.streams.len(), 1);
        assert_eq!(pipe.server.streams.len(), 1);

        // Server sends some data.
        assert_eq!(pipe.server.stream_send(4, b"goodbye, world", false), Ok(14));
        assert_eq!(pipe.advance(), Ok(()));

        // Server shuts down stream.
        assert_eq!(pipe.server.stream_shutdown(4, Shutdown::Write, 42), Ok(()));

        let mut r = pipe.server.writable();
        assert_eq!(r.next(), None);

        let (len, _) = pipe.server.send(&mut buf).unwrap();

        let mut dummy = buf[..len].to_vec();

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut dummy[..len]).unwrap();
        let mut iter = frames.iter();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::ResetStream {
                stream_id: 4,
                error_code: 42,
                final_size: 14,
            })
        );

        assert_eq!(pipe.client_recv(&mut buf[..len]), Ok(len));

        assert_eq!(pipe.advance(), Ok(()));

        // Sending more data is forbidden.
        assert_eq!(
            pipe.server.stream_send(4, b"bye", false),
            Err(Error::FinalSize)
        );

        // Client sends some data and closes the stream.
        assert_eq!(pipe.client.stream_send(4, b"bye", true), Ok(3));
        assert_eq!(pipe.advance(), Ok(()));

        // Server reads the data.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.server.stream_recv(4, &mut buf), Ok((15, true)));

        // Client processes readable streams.
        let mut r = pipe.client.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        assert_eq!(
            pipe.client.stream_recv(4, &mut buf),
            Err(Error::StreamReset(42))
        );

        // Stream is collected on both sides.
        assert_eq!(pipe.client.streams.len(), 0);
        assert_eq!(pipe.server.streams.len(), 0);

        assert_eq!(
            pipe.server.stream_shutdown(4, Shutdown::Write, 0),
            Err(Error::Done)
        );
    }

    #[test]
    /// Tests that shutting down a stream restores flow control for unsent data.
    fn stream_shutdown_write_unsent_tx_cap() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(15);
        config.set_initial_max_stream_data_bidi_local(30);
        config.set_initial_max_stream_data_bidi_remote(30);
        config.set_initial_max_stream_data_uni(30);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(0);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends some data.
        assert_eq!(pipe.client.stream_send(4, b"hello", true), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), None);

        let mut b = [0; 15];
        assert_eq!(pipe.server.stream_recv(4, &mut b), Ok((5, true)));

        // Server sends some data.
        assert_eq!(pipe.server.stream_send(4, b"hello", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Server buffers some data, until send capacity limit reached.
        assert_eq!(pipe.server.stream_send(4, b"hello", false), Ok(5));
        assert_eq!(pipe.server.stream_send(4, b"hello", false), Ok(5));
        assert_eq!(
            pipe.server.stream_send(4, b"hello", false),
            Err(Error::Done)
        );

        // Client shouldn't update flow control.
        assert!(!pipe.client.should_update_max_data());

        // Server shuts down stream.
        assert_eq!(pipe.server.stream_shutdown(4, Shutdown::Write, 42), Ok(()));
        assert_eq!(pipe.advance(), Ok(()));

        // Server can now send more data (on a different stream).
        assert_eq!(pipe.client.stream_send(8, b"hello", true), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.server.stream_send(8, b"hello", false), Ok(5));
        assert_eq!(pipe.server.stream_send(8, b"hello", false), Ok(5));
        assert_eq!(
            pipe.server.stream_send(8, b"hello", false),
            Err(Error::Done)
        );
        assert_eq!(pipe.advance(), Ok(()));
    }

    #[test]
    /// Tests that the order of flushable streams scheduled on the wire is the
    /// same as the order of `stream_send()` calls done by the application.
    fn stream_round_robin() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.stream_send(8, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.client.stream_send(0, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.client.stream_send(4, b"aaaaa", false), Ok(5));

        let (len, _) = pipe.client.send(&mut buf).unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.server, &mut buf[..len]).unwrap();

        let mut iter = frames.iter();

        // Skip ACK frame.
        iter.next();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::Stream {
                stream_id: 8,
                data: stream::RangeBuf::from(b"aaaaa", 0, false),
            })
        );

        let (len, _) = pipe.client.send(&mut buf).unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.server, &mut buf[..len]).unwrap();

        assert_eq!(
            frames.first(),
            Some(&frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"aaaaa", 0, false),
            })
        );

        let (len, _) = pipe.client.send(&mut buf).unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.server, &mut buf[..len]).unwrap();

        assert_eq!(
            frames.first(),
            Some(&frame::Frame::Stream {
                stream_id: 4,
                data: stream::RangeBuf::from(b"aaaaa", 0, false),
            })
        );
    }

    #[test]
    /// Tests the readable iterator.
    fn stream_readable() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // No readable streams.
        let mut r = pipe.client.readable();
        assert_eq!(r.next(), None);

        assert_eq!(pipe.client.stream_send(0, b"aaaaa", false), Ok(5));

        let mut r = pipe.client.readable();
        assert_eq!(r.next(), None);

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        assert_eq!(pipe.advance(), Ok(()));

        // Server received stream.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(
            pipe.server.stream_send(0, b"aaaaaaaaaaaaaaa", false),
            Ok(15)
        );
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.client.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        // Client drains stream.
        let mut b = [0; 15];
        pipe.client.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.client.readable();
        assert_eq!(r.next(), None);

        // Server shuts down stream.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.server.stream_shutdown(0, Shutdown::Read, 0), Ok(()));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        // Client creates multiple streams.
        assert_eq!(pipe.client.stream_send(4, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(8, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.server.readable();
        assert_eq!(r.len(), 2);

        assert!(r.next().is_some());
        assert!(r.next().is_some());
        assert!(r.next().is_none());

        assert_eq!(r.len(), 0);
    }

    #[test]
    /// Tests the writable iterator.
    fn stream_writable() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // No writable streams.
        let mut w = pipe.client.writable();
        assert_eq!(w.next(), None);

        assert_eq!(pipe.client.stream_send(0, b"aaaaa", false), Ok(5));

        // Client created stream.
        let mut w = pipe.client.writable();
        assert_eq!(w.next(), Some(0));
        assert_eq!(w.next(), None);

        assert_eq!(pipe.advance(), Ok(()));

        // Server created stream.
        let mut w = pipe.server.writable();
        assert_eq!(w.next(), Some(0));
        assert_eq!(w.next(), None);

        assert_eq!(
            pipe.server.stream_send(0, b"aaaaaaaaaaaaaaa", false),
            Ok(15)
        );

        // Server stream is full.
        let mut w = pipe.server.writable();
        assert_eq!(w.next(), None);

        assert_eq!(pipe.advance(), Ok(()));

        // Client drains stream.
        let mut b = [0; 15];
        pipe.client.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Server stream is writable again.
        let mut w = pipe.server.writable();
        assert_eq!(w.next(), Some(0));
        assert_eq!(w.next(), None);

        // Server shuts down stream.
        assert_eq!(pipe.server.stream_shutdown(0, Shutdown::Write, 0), Ok(()));

        let mut w = pipe.server.writable();
        assert_eq!(w.next(), None);

        // Client creates multiple streams.
        assert_eq!(pipe.client.stream_send(4, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(8, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        let mut w = pipe.server.writable();
        assert_eq!(w.len(), 2);

        assert!(w.next().is_some());
        assert!(w.next().is_some());
        assert!(w.next().is_none());

        assert_eq!(w.len(), 0);

        // Server finishes stream.
        assert_eq!(pipe.server.stream_send(8, b"aaaaa", true), Ok(5));

        let mut w = pipe.server.writable();
        assert_eq!(w.next(), Some(4));
        assert_eq!(w.next(), None);
    }

    #[test]
    fn stream_writable_blocked() {
        let mut config = crate::Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config.set_application_protos(&[b"h3"]).unwrap();
        config.set_initial_max_data(70);
        config.set_initial_max_stream_data_bidi_local(150000);
        config.set_initial_max_stream_data_bidi_remote(150000);
        config.set_initial_max_stream_data_uni(150000);
        config.set_initial_max_streams_bidi(100);
        config.set_initial_max_streams_uni(5);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client creates stream and sends some data.
        let send_buf = [0; 35];
        assert_eq!(pipe.client.stream_send(0, &send_buf, false), Ok(35));

        // Stream is still writable as it still has capacity.
        assert_eq!(pipe.client.stream_writable_next(), Some(0));
        assert_eq!(pipe.client.stream_writable_next(), None);

        // Client fills stream, which becomes unwritable due to connection
        // capacity.
        let send_buf = [0; 36];
        assert_eq!(pipe.client.stream_send(0, &send_buf, false), Ok(35));

        assert_eq!(pipe.client.stream_writable_next(), None);

        assert_eq!(pipe.client.tx_cap, 0);

        assert_eq!(pipe.advance(), Ok(()));

        let mut b = [0; 70];
        pipe.server.stream_recv(0, &mut b).unwrap();

        assert_eq!(pipe.advance(), Ok(()));

        // The connection capacity has increased and the stream is now writable
        // again.
        assert_ne!(pipe.client.tx_cap, 0);

        assert_eq!(pipe.client.stream_writable_next(), Some(0));
        assert_eq!(pipe.client.stream_writable_next(), None);
    }

    #[test]
    /// Tests that we don't exceed the per-connection flow control limit set by
    /// the peer.
    fn flow_control_limit_send() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(
            pipe.client.stream_send(0, b"aaaaaaaaaaaaaaa", false),
            Ok(15)
        );
        assert_eq!(pipe.advance(), Ok(()));
        assert_eq!(
            pipe.client.stream_send(4, b"aaaaaaaaaaaaaaa", false),
            Ok(15)
        );
        assert_eq!(pipe.advance(), Ok(()));
        assert_eq!(pipe.client.stream_send(8, b"a", false), Err(Error::Done));
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.server.readable();
        assert!(r.next().is_some());
        assert!(r.next().is_some());
        assert!(r.next().is_none());
    }

    #[test]
    /// Tests that invalid packets received before any other valid ones cause
    /// the server to close the connection immediately.
    fn invalid_initial_server() {
        let mut buf = [0; 65535];
        let mut pipe = testing::Pipe::new().unwrap();

        let frames = [frame::Frame::Padding { len: 10 }];

        let written = testing::encode_pkt(
            &mut pipe.client,
            packet::Type::Initial,
            &frames,
            &mut buf,
        )
        .unwrap();

        // Corrupt the packets's last byte to make decryption fail (the last
        // byte is part of the AEAD tag, so changing it means that the packet
        // cannot be authenticated during decryption).
        buf[written - 1] = !buf[written - 1];

        assert_eq!(pipe.server.timeout(), None);

        assert_eq!(
            pipe.server_recv(&mut buf[..written]),
            Err(Error::CryptoFail)
        );

        assert!(pipe.server.is_closed());
    }

    #[test]
    /// Tests that invalid Initial packets received to cause
    /// the client to close the connection immediately.
    fn invalid_initial_client() {
        let mut buf = [0; 65535];
        let mut pipe = testing::Pipe::new().unwrap();

        // Client sends initial flight.
        let (len, _) = pipe.client.send(&mut buf).unwrap();

        // Server sends initial flight.
        assert_eq!(pipe.server_recv(&mut buf[..len]), Ok(1200));

        let frames = [frame::Frame::Padding { len: 10 }];

        let written = testing::encode_pkt(
            &mut pipe.server,
            packet::Type::Initial,
            &frames,
            &mut buf,
        )
        .unwrap();

        // Corrupt the packets's last byte to make decryption fail (the last
        // byte is part of the AEAD tag, so changing it means that the packet
        // cannot be authenticated during decryption).
        buf[written - 1] = !buf[written - 1];

        // Client will ignore invalid packet.
        assert_eq!(pipe.client_recv(&mut buf[..written]), Ok(71));

        // The connection should be alive...
        assert!(!pipe.client.is_closed());

        // ...and the idle timeout should be armed.
        assert!(pipe.client.idle_timer.is_some());
    }

    #[test]
    /// Tests that packets with invalid payload length received before any other
    /// valid packet cause the server to close the connection immediately.
    fn invalid_initial_payload() {
        let mut buf = [0; 65535];
        let mut pipe = testing::Pipe::new().unwrap();

        let mut b = octets::OctetsMut::with_slice(&mut buf);

        let epoch = packet::Type::Initial.to_epoch().unwrap();

        let pn = 0;
        let pn_len = packet::pkt_num_len(pn, 0);

        let dcid = pipe.client.destination_id();
        let scid = pipe.client.source_id();

        let hdr = Header {
            ty: packet::Type::Initial,
            version: pipe.client.version,
            dcid: ConnectionId::from_ref(&dcid),
            scid: ConnectionId::from_ref(&scid),
            pkt_num: 0,
            pkt_num_len: pn_len,
            token: pipe.client.token.clone(),
            versions: None,
            key_phase: false,
        };

        hdr.to_bytes(&mut b).unwrap();

        // Payload length is invalid!!!
        let payload_len = 4096;

        let len = pn_len + payload_len;
        b.put_varint(len as u64).unwrap();

        packet::encode_pkt_num(pn, pn_len, &mut b).unwrap();

        let payload_offset = b.off();

        let frames = [frame::Frame::Padding { len: 10 }];

        for frame in &frames {
            frame.to_bytes(&mut b).unwrap();
        }

        let space = &mut pipe.client.pkt_num_spaces[epoch];

        // Use correct payload length when encrypting the packet.
        let payload_len = frames.iter().fold(0, |acc, x| acc + x.wire_len());

        let aead = space.crypto_seal.as_ref().unwrap();

        let written = packet::encrypt_pkt(
            &mut b,
            pn,
            pn_len,
            payload_len,
            payload_offset,
            None,
            aead,
        )
        .unwrap();

        assert_eq!(pipe.server.timeout(), None);

        assert_eq!(
            pipe.server_recv(&mut buf[..written]),
            Err(Error::InvalidPacket)
        );

        assert!(pipe.server.is_closed());
    }

    #[test]
    /// Tests that invalid packets don't cause the connection to be closed.
    fn invalid_packet() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = [frame::Frame::Padding { len: 10 }];

        let written = testing::encode_pkt(
            &mut pipe.client,
            packet::Type::Short,
            &frames,
            &mut buf,
        )
        .unwrap();

        // Corrupt the packets's last byte to make decryption fail (the last
        // byte is part of the AEAD tag, so changing it means that the packet
        // cannot be authenticated during decryption).
        buf[written - 1] = !buf[written - 1];

        assert_eq!(pipe.server_recv(&mut buf[..written]), Ok(written));

        // Corrupt the packets's first byte to make the header fail decoding.
        buf[0] = 255;

        assert_eq!(pipe.server_recv(&mut buf[..written]), Ok(written));
    }

    #[test]
    fn recv_empty_buffer() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.server_recv(&mut buf[..0]), Err(Error::BufferTooShort));
    }

    #[test]
    fn stop_sending_before_flushed_packets() {
        let mut b = [0; 15];

        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends some data, and closes stream.
        assert_eq!(pipe.client.stream_send(0, b"hello", true), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Server gets data.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.server.stream_recv(0, &mut b), Ok((5, true)));
        assert!(pipe.server.stream_finished(0));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        // Server sends data, until blocked.
        let mut r = pipe.server.writable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        while pipe.server.stream_send(0, b"world", false) != Err(Error::Done) {}

        let mut r = pipe.server.writable();
        assert_eq!(r.next(), None);

        // Client sends STOP_SENDING.
        let frames = [frame::Frame::StopSending {
            stream_id: 0,
            error_code: 42,
        }];

        let pkt_type = packet::Type::Short;
        let len = pipe
            .send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();

        // Server sent a RESET_STREAM frame in response.
        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();

        let mut iter = frames.iter();

        // Skip ACK frame.
        iter.next();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::ResetStream {
                stream_id: 0,
                error_code: 42,
                final_size: 0,
            })
        );

        // Stream is writable, but writing returns an error.
        let mut r = pipe.server.writable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(
            pipe.server.stream_send(0, b"world", true),
            Err(Error::StreamStopped(42)),
        );

        assert_eq!(pipe.server.streams.len(), 1);

        // Client acks RESET_STREAM frame.
        let mut ranges = ranges::RangeSet::default();
        ranges.insert(0..6);

        let frames = [frame::Frame::ACK {
            ack_delay: 15,
            ranges,
            ecn_counts: None,
        }];

        assert_eq!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf), Ok(0));

        // Client has ACK'd the RESET_STREAM so the stream is collected.
        assert_eq!(pipe.server.streams.len(), 0);
    }

    #[test]
    fn reset_before_flushed_packets() {
        let mut b = [0; 15];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(5);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_streams_bidi(3);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends some data, and closes stream.
        assert_eq!(pipe.client.stream_send(0, b"hello", true), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Server gets data.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.server.stream_recv(0, &mut b), Ok((5, true)));
        assert!(pipe.server.stream_finished(0));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        // Server sends data and is blocked by small stream flow control.
        let mut r = pipe.server.writable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        assert_eq!(pipe.server.stream_send(0, b"helloworld", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Client reads to give flow control back.
        assert_eq!(pipe.client.stream_recv(0, &mut b), Ok((5, false)));
        assert_eq!(pipe.advance(), Ok(()));

        // Server writes stream data and resets the stream before sending a
        // packet.
        assert_eq!(pipe.server.stream_send(0, b"world", false), Ok(5));
        pipe.server.stream_shutdown(0, Shutdown::Write, 42).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Client has ACK'd the RESET_STREAM so the stream is collected.
        assert_eq!(pipe.server.streams.len(), 0);
    }

    #[test]
    /// Tests that the MAX_STREAMS frame is sent for bidirectional streams.
    fn stream_limit_update_bidi() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(0);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends stream data.
        assert_eq!(pipe.client.stream_send(0, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(4, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(4, b"b", true), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(0, b"b", true), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        // Server reads stream data.
        let mut b = [0; 15];
        pipe.server.stream_recv(0, &mut b).unwrap();
        pipe.server.stream_recv(4, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Server sends stream data, with fin.
        assert_eq!(pipe.server.stream_send(0, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.server.stream_send(4, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.server.stream_send(4, b"b", true), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.server.stream_send(0, b"b", true), Ok(1));

        // Server sends MAX_STREAMS.
        assert_eq!(pipe.advance(), Ok(()));

        // Client tries to create new streams.
        assert_eq!(pipe.client.stream_send(8, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(12, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(16, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(
            pipe.client.stream_send(20, b"a", false),
            Err(Error::StreamLimit)
        );

        assert_eq!(pipe.server.readable().len(), 3);
    }

    #[test]
    /// Tests that the MAX_STREAMS frame is sent for unidirectional streams.
    fn stream_limit_update_uni() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(0);
        config.set_initial_max_streams_uni(3);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends stream data.
        assert_eq!(pipe.client.stream_send(2, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(6, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(6, b"b", true), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(2, b"b", true), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        // Server reads stream data.
        let mut b = [0; 15];
        pipe.server.stream_recv(2, &mut b).unwrap();
        pipe.server.stream_recv(6, &mut b).unwrap();

        // Server sends MAX_STREAMS.
        assert_eq!(pipe.advance(), Ok(()));

        // Client tries to create new streams.
        assert_eq!(pipe.client.stream_send(10, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(14, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(18, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(
            pipe.client.stream_send(22, b"a", false),
            Err(Error::StreamLimit)
        );

        assert_eq!(pipe.server.readable().len(), 3);
    }

    #[test]
    /// Tests that the stream's fin flag is properly flushed even if there's no
    /// data in the buffer, and that the buffer becomes readable on the other
    /// side.
    fn stream_zero_length_fin() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(
            pipe.client.stream_send(0, b"aaaaaaaaaaaaaaa", false),
            Ok(15)
        );
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert!(r.next().is_none());

        let mut b = [0; 15];
        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Client sends zero-length frame.
        assert_eq!(pipe.client.stream_send(0, b"", true), Ok(0));
        assert_eq!(pipe.advance(), Ok(()));

        // Stream should be readable on the server after receiving empty fin.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert!(r.next().is_none());

        let mut b = [0; 15];
        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Client sends zero-length frame (again).
        assert_eq!(pipe.client.stream_send(0, b"", true), Ok(0));
        assert_eq!(pipe.advance(), Ok(()));

        // Stream should _not_ be readable on the server after receiving empty
        // fin, because it was already finished.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);
    }

    #[test]
    /// Tests that the stream's fin flag is properly flushed even if there's no
    /// data in the buffer, that the buffer becomes readable on the other
    /// side and stays readable even if the stream is fin'd locally.
    fn stream_zero_length_fin_deferred_collection() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(
            pipe.client.stream_send(0, b"aaaaaaaaaaaaaaa", false),
            Ok(15)
        );
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert!(r.next().is_none());

        let mut b = [0; 15];
        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Client sends zero-length frame.
        assert_eq!(pipe.client.stream_send(0, b"", true), Ok(0));
        assert_eq!(pipe.advance(), Ok(()));

        // Server sends zero-length frame.
        assert_eq!(pipe.server.stream_send(0, b"", true), Ok(0));
        assert_eq!(pipe.advance(), Ok(()));

        // Stream should be readable on the server after receiving empty fin.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert!(r.next().is_none());

        let mut b = [0; 15];
        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Client sends zero-length frame (again).
        assert_eq!(pipe.client.stream_send(0, b"", true), Ok(0));
        assert_eq!(pipe.advance(), Ok(()));

        // Stream should _not_ be readable on the server after receiving empty
        // fin, because it was already finished.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), None);

        // Stream _is_readable on the client side.
        let mut r = pipe.client.readable();
        assert_eq!(r.next(), Some(0));

        pipe.client.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Stream is completed and _is not_ readable.
        let mut r = pipe.client.readable();
        assert_eq!(r.next(), None);
    }

    #[test]
    /// Tests that the stream gets created with stream_send() even if there's
    /// no data in the buffer and the fin flag is not set.
    fn stream_zero_length_non_fin() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.stream_send(0, b"", false), Ok(0));

        // The stream now should have been created.
        assert_eq!(pipe.client.streams.len(), 1);
        assert_eq!(pipe.advance(), Ok(()));

        // Sending an empty non-fin should not change any stream state on the
        // other side.
        let mut r = pipe.server.readable();
        assert!(r.next().is_none());
    }

    #[test]
    /// Tests that completed streams are garbage collected.
    fn collect_streams() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.streams.len(), 0);
        assert_eq!(pipe.server.streams.len(), 0);

        assert_eq!(pipe.client.stream_send(0, b"aaaaa", true), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        assert!(!pipe.client.stream_finished(0));
        assert!(!pipe.server.stream_finished(0));

        assert_eq!(pipe.client.streams.len(), 1);
        assert_eq!(pipe.server.streams.len(), 1);

        let mut b = [0; 5];
        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.server.stream_send(0, b"aaaaa", true), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        assert!(!pipe.client.stream_finished(0));
        assert!(pipe.server.stream_finished(0));

        assert_eq!(pipe.client.streams.len(), 1);
        assert_eq!(pipe.server.streams.len(), 0);

        let mut b = [0; 5];
        pipe.client.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.streams.len(), 0);
        assert_eq!(pipe.server.streams.len(), 0);

        assert!(pipe.client.stream_finished(0));
        assert!(pipe.server.stream_finished(0));

        assert_eq!(pipe.client.stream_send(0, b"", true), Err(Error::Done));

        let frames = [frame::Frame::Stream {
            stream_id: 0,
            data: stream::RangeBuf::from(b"aa", 0, false),
        }];

        let pkt_type = packet::Type::Short;
        assert_eq!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf), Ok(39));
    }

    #[test]
    fn config_set_cc_algorithm_name() {
        let mut config = Config::new(PROTOCOL_VERSION).unwrap();

        assert_eq!(config.set_cc_algorithm_name("reno"), Ok(()));

        // Unknown name.
        assert_eq!(
            config.set_cc_algorithm_name("???"),
            Err(Error::CongestionControl)
        );
    }

    #[test]
    fn peer_cert() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        match pipe.client.peer_cert() {
            Some(c) => assert_eq!(c.len(), 753),

            None => panic!("missing server certificate"),
        }
    }

    #[test]
    fn peer_cert_chain() {
        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert-big.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();

        let mut pipe = testing::Pipe::with_server_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        match pipe.client.peer_cert_chain() {
            Some(c) => assert_eq!(c.len(), 5),

            None => panic!("missing server certificate chain"),
        }
    }

    #[test]
    fn retry() {
        let mut buf = [0; 65535];

        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();

        let mut pipe = testing::Pipe::with_server_config(&mut config).unwrap();

        // Client sends initial flight.
        let (mut len, _) = pipe.client.send(&mut buf).unwrap();

        // Server sends Retry packet.
        let hdr = Header::from_slice(&mut buf[..len], MAX_CONN_ID_LEN).unwrap();

        let odcid = hdr.dcid.clone();

        let mut scid = [0; MAX_CONN_ID_LEN];
        rand::rand_bytes(&mut scid[..]);
        let scid = ConnectionId::from_ref(&scid);

        let token = b"quiche test retry token";

        len = packet::retry(
            &hdr.scid,
            &hdr.dcid,
            &scid,
            token,
            hdr.version,
            &mut buf,
        )
        .unwrap();

        // Client receives Retry and sends new Initial.
        assert_eq!(pipe.client_recv(&mut buf[..len]), Ok(len));

        let (len, _) = pipe.client.send(&mut buf).unwrap();

        let hdr = Header::from_slice(&mut buf[..len], MAX_CONN_ID_LEN).unwrap();
        assert_eq!(&hdr.token.unwrap(), token);

        // Server accepts connection.
        let from = "127.0.0.1:1234".parse().unwrap();
        pipe.server = accept(
            &scid,
            Some(&odcid),
            testing::Pipe::server_addr(),
            from,
            &mut config,
        )
        .unwrap();
        assert_eq!(pipe.server_recv(&mut buf[..len]), Ok(len));

        assert_eq!(pipe.advance(), Ok(()));

        assert!(pipe.client.is_established());
        assert!(pipe.server.is_established());
    }

    #[test]
    fn missing_retry_source_connection_id() {
        let mut buf = [0; 65535];

        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();

        let mut pipe = testing::Pipe::with_server_config(&mut config).unwrap();

        // Client sends initial flight.
        let (mut len, _) = pipe.client.send(&mut buf).unwrap();

        // Server sends Retry packet.
        let hdr = Header::from_slice(&mut buf[..len], MAX_CONN_ID_LEN).unwrap();

        let mut scid = [0; MAX_CONN_ID_LEN];
        rand::rand_bytes(&mut scid[..]);
        let scid = ConnectionId::from_ref(&scid);

        let token = b"quiche test retry token";

        len = packet::retry(
            &hdr.scid,
            &hdr.dcid,
            &scid,
            token,
            hdr.version,
            &mut buf,
        )
        .unwrap();

        // Client receives Retry and sends new Initial.
        assert_eq!(pipe.client_recv(&mut buf[..len]), Ok(len));

        let (len, _) = pipe.client.send(&mut buf).unwrap();

        // Server accepts connection and send first flight. But original
        // destination connection ID is ignored.
        let from = "127.0.0.1:1234".parse().unwrap();
        pipe.server =
            accept(&scid, None, testing::Pipe::server_addr(), from, &mut config)
                .unwrap();
        assert_eq!(pipe.server_recv(&mut buf[..len]), Ok(len));

        let flight = testing::emit_flight(&mut pipe.server).unwrap();

        assert_eq!(
            testing::process_flight(&mut pipe.client, flight),
            Err(Error::InvalidTransportParam)
        );
    }

    #[test]
    fn invalid_retry_source_connection_id() {
        let mut buf = [0; 65535];

        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();

        let mut pipe = testing::Pipe::with_server_config(&mut config).unwrap();

        // Client sends initial flight.
        let (mut len, _) = pipe.client.send(&mut buf).unwrap();

        // Server sends Retry packet.
        let hdr = Header::from_slice(&mut buf[..len], MAX_CONN_ID_LEN).unwrap();

        let mut scid = [0; MAX_CONN_ID_LEN];
        rand::rand_bytes(&mut scid[..]);
        let scid = ConnectionId::from_ref(&scid);

        let token = b"quiche test retry token";

        len = packet::retry(
            &hdr.scid,
            &hdr.dcid,
            &scid,
            token,
            hdr.version,
            &mut buf,
        )
        .unwrap();

        // Client receives Retry and sends new Initial.
        assert_eq!(pipe.client_recv(&mut buf[..len]), Ok(len));

        let (len, _) = pipe.client.send(&mut buf).unwrap();

        // Server accepts connection and send first flight. But original
        // destination connection ID is invalid.
        let from = "127.0.0.1:1234".parse().unwrap();
        let odcid = ConnectionId::from_ref(b"bogus value");
        pipe.server = accept(
            &scid,
            Some(&odcid),
            testing::Pipe::server_addr(),
            from,
            &mut config,
        )
        .unwrap();
        assert_eq!(pipe.server_recv(&mut buf[..len]), Ok(len));

        let flight = testing::emit_flight(&mut pipe.server).unwrap();

        assert_eq!(
            testing::process_flight(&mut pipe.client, flight),
            Err(Error::InvalidTransportParam)
        );
    }

    #[test]
    /// Tests that a zero-length NEW_TOKEN frame is detected as an error.
    fn zero_length_new_token() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = vec![frame::Frame::NewToken { token: vec![] }];

        let pkt_type = packet::Type::Short;

        let written =
            testing::encode_pkt(&mut pipe.server, pkt_type, &frames, &mut buf)
                .unwrap();

        assert_eq!(
            pipe.client_recv(&mut buf[..written]),
            Err(Error::InvalidFrame)
        );
    }

    #[test]
    /// Tests that a NEW_TOKEN frame sent by client is detected as an error.
    fn client_sent_new_token() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let frames = vec![frame::Frame::NewToken {
            token: vec![1, 2, 3],
        }];

        let pkt_type = packet::Type::Short;

        let written =
            testing::encode_pkt(&mut pipe.client, pkt_type, &frames, &mut buf)
                .unwrap();

        assert_eq!(
            pipe.server_recv(&mut buf[..written]),
            Err(Error::InvalidPacket)
        );
    }

    fn check_send(_: &mut impl Send) {}

    #[test]
    fn config_must_be_send() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        check_send(&mut config);
    }

    #[test]
    fn connection_must_be_send() {
        let mut pipe = testing::Pipe::new().unwrap();
        check_send(&mut pipe.client);
    }

    fn check_sync(_: &mut impl Sync) {}

    #[test]
    fn config_must_be_sync() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        check_sync(&mut config);
    }

    #[test]
    fn connection_must_be_sync() {
        let mut pipe = testing::Pipe::new().unwrap();
        check_sync(&mut pipe.client);
    }

    #[test]
    fn data_blocked() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.stream_send(0, b"aaaaaaaaaa", false), Ok(10));
        assert_eq!(pipe.client.blocked_limit, None);
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(4, b"aaaaaaaaaa", false), Ok(10));
        assert_eq!(pipe.client.blocked_limit, None);
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(8, b"aaaaaaaaaaa", false), Ok(10));
        assert_eq!(pipe.client.blocked_limit, Some(30));

        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(pipe.client.blocked_limit, None);

        let frames =
            testing::decode_pkt(&mut pipe.server, &mut buf[..len]).unwrap();

        let mut iter = frames.iter();

        assert_eq!(iter.next(), Some(&frame::Frame::DataBlocked { limit: 30 }));

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::Stream {
                stream_id: 8,
                data: stream::RangeBuf::from(b"aaaaaaaaaa", 0, false),
            })
        );

        assert_eq!(iter.next(), None);
    }

    #[test]
    fn stream_data_blocked() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.stream_send(0, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.client.streams.blocked().len(), 0);

        assert_eq!(pipe.client.stream_send(0, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.client.streams.blocked().len(), 0);

        assert_eq!(pipe.client.stream_send(0, b"aaaaaa", false), Ok(5));
        assert_eq!(pipe.client.streams.blocked().len(), 1);

        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(pipe.client.streams.blocked().len(), 0);

        let frames =
            testing::decode_pkt(&mut pipe.server, &mut buf[..len]).unwrap();

        let mut iter = frames.iter();

        // Skip ACK frame.
        iter.next();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::StreamDataBlocked {
                stream_id: 0,
                limit: 15,
            })
        );

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"aaaaaaaaaaaaaaa", 0, false),
            })
        );

        assert_eq!(iter.next(), None);

        // Send from another stream, make sure we don't send STREAM_DATA_BLOCKED
        // again.
        assert_eq!(pipe.client.stream_send(4, b"a", false), Ok(1));

        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(pipe.client.streams.blocked().len(), 0);

        let frames =
            testing::decode_pkt(&mut pipe.server, &mut buf[..len]).unwrap();

        let mut iter = frames.iter();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::Stream {
                stream_id: 4,
                data: stream::RangeBuf::from(b"a", 0, false),
            })
        );

        assert_eq!(iter.next(), None);

        // Send again from blocked stream and make sure it is not marked as
        // blocked again.
        assert_eq!(
            pipe.client.stream_send(0, b"aaaaaa", false),
            Err(Error::Done)
        );
        assert_eq!(pipe.client.streams.blocked().len(), 0);
        assert_eq!(pipe.client.send(&mut buf), Err(Error::Done));
    }

    #[test]
    fn stream_data_blocked_unblocked_flow_control() {
        let mut buf = [0; 65535];
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(
            pipe.client.stream_send(0, b"aaaaaaaaaaaaaaah", false),
            Ok(15)
        );
        assert_eq!(pipe.client.streams.blocked().len(), 1);
        assert_eq!(pipe.advance(), Ok(()));
        assert_eq!(pipe.client.streams.blocked().len(), 0);

        // Send again on blocked stream. It's blocked at the same offset as
        // previously, so it should not be marked as blocked again.
        assert_eq!(pipe.client.stream_send(0, b"h", false), Err(Error::Done));
        assert_eq!(pipe.client.streams.blocked().len(), 0);

        // No matter how many times we try to write stream data tried, no
        // packets containing STREAM_BLOCKED should be emitted.
        assert_eq!(pipe.client.stream_send(0, b"h", false), Err(Error::Done));
        assert_eq!(pipe.client.send(&mut buf), Err(Error::Done));

        assert_eq!(pipe.client.stream_send(0, b"h", false), Err(Error::Done));
        assert_eq!(pipe.client.send(&mut buf), Err(Error::Done));

        assert_eq!(pipe.client.stream_send(0, b"h", false), Err(Error::Done));
        assert_eq!(pipe.client.send(&mut buf), Err(Error::Done));

        // Now read some data at the server to release flow control.
        let mut r = pipe.server.readable();
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), None);

        let mut b = [0; 10];
        assert_eq!(pipe.server.stream_recv(0, &mut b), Ok((10, false)));
        assert_eq!(&b[..10], b"aaaaaaaaaa");
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(0, b"hhhhhhhhhh!", false), Ok(10));
        assert_eq!(pipe.client.streams.blocked().len(), 1);

        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(pipe.client.streams.blocked().len(), 0);

        let frames =
            testing::decode_pkt(&mut pipe.server, &mut buf[..len]).unwrap();

        let mut iter = frames.iter();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::StreamDataBlocked {
                stream_id: 0,
                limit: 25,
            })
        );

        // don't care about remaining received frames

        assert_eq!(pipe.client.stream_send(0, b"!", false), Err(Error::Done));
        assert_eq!(pipe.client.streams.blocked().len(), 0);
        assert_eq!(pipe.client.send(&mut buf), Err(Error::Done));
    }

    #[test]
    fn app_limited_true() {
        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(50000);
        config.set_initial_max_stream_data_bidi_local(50000);
        config.set_initial_max_stream_data_bidi_remote(50000);
        config.set_max_recv_udp_payload_size(1200);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_client_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends stream data.
        assert_eq!(pipe.client.stream_send(0, b"a", true), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        // Server reads stream data.
        let mut b = [0; 15];
        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Server sends stream data smaller than cwnd.
        let send_buf = [0; 10000];
        assert_eq!(pipe.server.stream_send(0, &send_buf, false), Ok(10000));
        assert_eq!(pipe.advance(), Ok(()));

        // app_limited should be true because we send less than cwnd.
        assert!(pipe
            .server
            .paths
            .get_active()
            .expect("no active")
            .recovery
            .app_limited());
    }

    #[test]
    fn app_limited_false() {
        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(50000);
        config.set_initial_max_stream_data_bidi_local(50000);
        config.set_initial_max_stream_data_bidi_remote(50000);
        config.set_max_recv_udp_payload_size(1200);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_client_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends stream data.
        assert_eq!(pipe.client.stream_send(0, b"a", true), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        // Server reads stream data.
        let mut b = [0; 15];
        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Server sends stream data bigger than cwnd.
        let send_buf1 = [0; 20000];
        assert_eq!(pipe.server.stream_send(0, &send_buf1, false), Ok(12000));

        testing::emit_flight(&mut pipe.server).ok();

        // We can't create a new packet header because there is no room by cwnd.
        // app_limited should be false because we can't send more by cwnd.
        assert!(!pipe
            .server
            .paths
            .get_active()
            .expect("no active")
            .recovery
            .app_limited());
    }

    #[test]
    fn sends_ack_only_pkt_when_full_cwnd_and_ack_elicited() {
        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(50000);
        config.set_initial_max_stream_data_bidi_local(50000);
        config.set_initial_max_stream_data_bidi_remote(50000);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(3);
        config.set_max_recv_udp_payload_size(1200);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends stream data bigger than cwnd (it will never arrive to the
        // server).
        let send_buf1 = [0; 20000];
        assert_eq!(pipe.client.stream_send(0, &send_buf1, false), Ok(12000));

        testing::emit_flight(&mut pipe.client).ok();

        // Server sends some stream data that will need ACKs.
        assert_eq!(
            pipe.server.stream_send(1, &send_buf1[..500], false),
            Ok(500)
        );

        testing::process_flight(
            &mut pipe.client,
            testing::emit_flight(&mut pipe.server).unwrap(),
        )
        .unwrap();

        let mut buf = [0; 2000];

        let ret = pipe.client.send(&mut buf);

        assert_eq!(pipe.client.tx_cap, 0);

        assert!(matches!(ret, Ok((_, _))), "the client should at least send one packet to acknowledge the newly received data");

        let (sent, _) = ret.unwrap();

        assert_ne!(sent, 0, "the client should at least send a pure ACK packet");

        let frames =
            testing::decode_pkt(&mut pipe.server, &mut buf[..sent]).unwrap();
        assert_eq!(1, frames.len());
        assert!(
            matches!(frames[0], frame::Frame::ACK { .. }),
            "the packet sent by the client must be an ACK only packet"
        );
    }

    /// Like sends_ack_only_pkt_when_full_cwnd_and_ack_elicited, but when
    /// ack_eliciting is explicitly requested.
    #[test]
    fn sends_ack_only_pkt_when_full_cwnd_and_ack_elicited_despite_max_unacknowledging(
    ) {
        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(50000);
        config.set_initial_max_stream_data_bidi_local(50000);
        config.set_initial_max_stream_data_bidi_remote(50000);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(3);
        config.set_max_recv_udp_payload_size(1200);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends stream data bigger than cwnd (it will never arrive to the
        // server). This exhausts the congestion window.
        let send_buf1 = [0; 20000];
        assert_eq!(pipe.client.stream_send(0, &send_buf1, false), Ok(12000));

        testing::emit_flight(&mut pipe.client).ok();

        // Client gets PING frames from server, which elicit ACK
        let mut buf = [0; 2000];
        for _ in 0..recovery::MAX_OUTSTANDING_NON_ACK_ELICITING {
            let written = testing::encode_pkt(
                &mut pipe.server,
                packet::Type::Short,
                &[frame::Frame::Ping { mtu_probe: None }],
                &mut buf,
            )
            .unwrap();

            pipe.client_recv(&mut buf[..written])
                .expect("client recv ping");

            // Client acknowledges despite a full congestion window
            let ret = pipe.client.send(&mut buf);

            assert!(matches!(ret, Ok((_, _))), "the client should at least send one packet to acknowledge the newly received data");

            let (sent, _) = ret.unwrap();

            assert_ne!(
                sent, 0,
                "the client should at least send a pure ACK packet"
            );

            let frames =
                testing::decode_pkt(&mut pipe.server, &mut buf[..sent]).unwrap();

            assert_eq!(1, frames.len());

            assert!(
                matches!(frames[0], frame::Frame::ACK { .. }),
                "the packet sent by the client must be an ACK only packet"
            );
        }

        // The client shouldn't need to send any more packets after the ACK only
        // packet it just sent.
        assert_eq!(
            pipe.client.send(&mut buf),
            Err(Error::Done),
            "nothing for client to send after ACK-only packet"
        );
    }

    #[test]
    fn app_limited_false_no_frame() {
        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(50000);
        config.set_initial_max_stream_data_bidi_local(50000);
        config.set_initial_max_stream_data_bidi_remote(50000);
        config.set_max_recv_udp_payload_size(1405);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_client_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends stream data.
        assert_eq!(pipe.client.stream_send(0, b"a", true), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        // Server reads stream data.
        let mut b = [0; 15];
        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Server sends stream data bigger than cwnd.
        let send_buf1 = [0; 20000];
        assert_eq!(pipe.server.stream_send(0, &send_buf1, false), Ok(12000));

        testing::emit_flight(&mut pipe.server).ok();

        // We can't create a new packet header because there is no room by cwnd.
        // app_limited should be false because we can't send more by cwnd.
        assert!(!pipe
            .server
            .paths
            .get_active()
            .expect("no active")
            .recovery
            .app_limited());
    }

    #[test]
    fn app_limited_false_no_header() {
        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(50000);
        config.set_initial_max_stream_data_bidi_local(50000);
        config.set_initial_max_stream_data_bidi_remote(50000);
        config.set_max_recv_udp_payload_size(1406);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_client_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends stream data.
        assert_eq!(pipe.client.stream_send(0, b"a", true), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        // Server reads stream data.
        let mut b = [0; 15];
        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Server sends stream data bigger than cwnd.
        let send_buf1 = [0; 20000];
        assert_eq!(pipe.server.stream_send(0, &send_buf1, false), Ok(12000));

        testing::emit_flight(&mut pipe.server).ok();

        // We can't create a new frame because there is no room by cwnd.
        // app_limited should be false because we can't send more by cwnd.
        assert!(!pipe
            .server
            .paths
            .get_active()
            .expect("no active")
            .recovery
            .app_limited());
    }

    #[test]
    fn app_limited_not_changed_on_no_new_frames() {
        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(50000);
        config.set_initial_max_stream_data_bidi_local(50000);
        config.set_initial_max_stream_data_bidi_remote(50000);
        config.set_max_recv_udp_payload_size(1200);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_client_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends stream data.
        assert_eq!(pipe.client.stream_send(0, b"a", true), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        // Server reads stream data.
        let mut b = [0; 15];
        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        // Client's app_limited is true because its bytes-in-flight
        // is much smaller than the current cwnd.
        assert!(pipe
            .client
            .paths
            .get_active()
            .expect("no active")
            .recovery
            .app_limited());

        // Client has no new frames to send - returns Done.
        assert_eq!(testing::emit_flight(&mut pipe.client), Err(Error::Done));

        // Client's app_limited should remain the same.
        assert!(pipe
            .client
            .paths
            .get_active()
            .expect("no active")
            .recovery
            .app_limited());
    }

    #[test]
    fn limit_ack_ranges() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let epoch = packet::Epoch::Application;

        assert_eq!(pipe.server.pkt_num_spaces[epoch].recv_pkt_need_ack.len(), 0);

        let frames = [
            frame::Frame::Ping { mtu_probe: None },
            frame::Frame::Padding { len: 3 },
        ];

        let pkt_type = packet::Type::Short;

        let mut last_packet_sent = 0;

        for _ in 0..512 {
            let recv_count = pipe.server.recv_count;

            last_packet_sent = pipe.client.pkt_num_spaces[epoch].next_pkt_num;

            pipe.send_pkt_to_server(pkt_type, &frames, &mut buf)
                .unwrap();

            assert_eq!(pipe.server.recv_count, recv_count + 1);

            // Skip packet number.
            pipe.client.pkt_num_spaces[epoch].next_pkt_num += 1;
        }

        assert_eq!(
            pipe.server.pkt_num_spaces[epoch].recv_pkt_need_ack.len(),
            MAX_ACK_RANGES
        );

        assert_eq!(
            pipe.server.pkt_num_spaces[epoch].recv_pkt_need_ack.first(),
            Some(last_packet_sent - ((MAX_ACK_RANGES as u64) - 1) * 2)
        );

        assert_eq!(
            pipe.server.pkt_num_spaces[epoch].recv_pkt_need_ack.last(),
            Some(last_packet_sent)
        );
    }

    #[test]
    /// Tests that streams are correctly scheduled based on their priority.
    fn stream_priority() {
        // Limit 1-RTT packet size to avoid congestion control interference.
        const MAX_TEST_PACKET_SIZE: usize = 540;

        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(1_000_000);
        config.set_initial_max_stream_data_bidi_local(1_000_000);
        config.set_initial_max_stream_data_bidi_remote(1_000_000);
        config.set_initial_max_stream_data_uni(0);
        config.set_initial_max_streams_bidi(100);
        config.set_initial_max_streams_uni(0);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.stream_send(0, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(4, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(8, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(12, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(16, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(20, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        let mut b = [0; 1];

        let out = [b'b'; 500];

        // Server prioritizes streams as follows:
        //  * Stream 8 and 16 have the same priority but are non-incremental.
        //  * Stream 4, 12 and 20 have the same priority but 20 is non-incremental
        //    and 4 and 12 are incremental.
        //  * Stream 0 is on its own.

        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.server.stream_priority(0, 255, true), Ok(()));
        pipe.server.stream_send(0, &out, false).unwrap();
        pipe.server.stream_send(0, &out, false).unwrap();
        pipe.server.stream_send(0, &out, false).unwrap();

        pipe.server.stream_recv(12, &mut b).unwrap();
        assert_eq!(pipe.server.stream_priority(12, 42, true), Ok(()));
        pipe.server.stream_send(12, &out, false).unwrap();
        pipe.server.stream_send(12, &out, false).unwrap();
        pipe.server.stream_send(12, &out, false).unwrap();

        pipe.server.stream_recv(16, &mut b).unwrap();
        assert_eq!(pipe.server.stream_priority(16, 10, false), Ok(()));
        pipe.server.stream_send(16, &out, false).unwrap();
        pipe.server.stream_send(16, &out, false).unwrap();
        pipe.server.stream_send(16, &out, false).unwrap();

        pipe.server.stream_recv(4, &mut b).unwrap();
        assert_eq!(pipe.server.stream_priority(4, 42, true), Ok(()));
        pipe.server.stream_send(4, &out, false).unwrap();
        pipe.server.stream_send(4, &out, false).unwrap();
        pipe.server.stream_send(4, &out, false).unwrap();

        pipe.server.stream_recv(8, &mut b).unwrap();
        assert_eq!(pipe.server.stream_priority(8, 10, false), Ok(()));
        pipe.server.stream_send(8, &out, false).unwrap();
        pipe.server.stream_send(8, &out, false).unwrap();
        pipe.server.stream_send(8, &out, false).unwrap();

        pipe.server.stream_recv(20, &mut b).unwrap();
        assert_eq!(pipe.server.stream_priority(20, 42, false), Ok(()));
        pipe.server.stream_send(20, &out, false).unwrap();
        pipe.server.stream_send(20, &out, false).unwrap();
        pipe.server.stream_send(20, &out, false).unwrap();

        // First is stream 8.
        let mut off = 0;

        for _ in 1..=3 {
            let (len, _) =
                pipe.server.send(&mut buf[..MAX_TEST_PACKET_SIZE]).unwrap();

            let frames =
                testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
            let stream = frames.first().unwrap();

            assert_eq!(stream, &frame::Frame::Stream {
                stream_id: 8,
                data: stream::RangeBuf::from(&out, off, false),
            });

            off = match stream {
                frame::Frame::Stream { data, .. } => data.max_off(),

                _ => unreachable!(),
            };
        }

        // Then is stream 16.
        let mut off = 0;

        for _ in 1..=3 {
            let (len, _) =
                pipe.server.send(&mut buf[..MAX_TEST_PACKET_SIZE]).unwrap();

            let frames =
                testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
            let stream = frames.first().unwrap();

            assert_eq!(stream, &frame::Frame::Stream {
                stream_id: 16,
                data: stream::RangeBuf::from(&out, off, false),
            });

            off = match stream {
                frame::Frame::Stream { data, .. } => data.max_off(),

                _ => unreachable!(),
            };
        }

        // Then is stream 20.
        let mut off = 0;

        for _ in 1..=3 {
            let (len, _) =
                pipe.server.send(&mut buf[..MAX_TEST_PACKET_SIZE]).unwrap();

            let frames =
                testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
            let stream = frames.first().unwrap();

            assert_eq!(stream, &frame::Frame::Stream {
                stream_id: 20,
                data: stream::RangeBuf::from(&out, off, false),
            });

            off = match stream {
                frame::Frame::Stream { data, .. } => data.max_off(),

                _ => unreachable!(),
            };
        }

        // Then are stream 12 and 4, with the same priority, incrementally.
        let mut off = 0;

        for _ in 1..=3 {
            let (len, _) =
                pipe.server.send(&mut buf[..MAX_TEST_PACKET_SIZE]).unwrap();

            let frames =
                testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();

            assert_eq!(
                frames.first(),
                Some(&frame::Frame::Stream {
                    stream_id: 12,
                    data: stream::RangeBuf::from(&out, off, false),
                })
            );

            let (len, _) =
                pipe.server.send(&mut buf[..MAX_TEST_PACKET_SIZE]).unwrap();

            let frames =
                testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();

            let stream = frames.first().unwrap();

            assert_eq!(stream, &frame::Frame::Stream {
                stream_id: 4,
                data: stream::RangeBuf::from(&out, off, false),
            });

            off = match stream {
                frame::Frame::Stream { data, .. } => data.max_off(),

                _ => unreachable!(),
            };
        }

        // Final is stream 0.
        let mut off = 0;

        for _ in 1..=3 {
            let (len, _) =
                pipe.server.send(&mut buf[..MAX_TEST_PACKET_SIZE]).unwrap();

            let frames =
                testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
            let stream = frames.first().unwrap();

            assert_eq!(stream, &frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(&out, off, false),
            });

            off = match stream {
                frame::Frame::Stream { data, .. } => data.max_off(),

                _ => unreachable!(),
            };
        }

        assert_eq!(pipe.server.send(&mut buf), Err(Error::Done));
    }

    #[test]
    /// Tests that changing a stream's priority is correctly propagated.
    fn stream_reprioritize() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(0);
        config.set_initial_max_streams_bidi(5);
        config.set_initial_max_streams_uni(0);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.stream_send(0, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(4, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(8, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(12, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        let mut b = [0; 1];

        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.server.stream_priority(0, 255, true), Ok(()));
        pipe.server.stream_send(0, b"b", false).unwrap();

        pipe.server.stream_recv(12, &mut b).unwrap();
        assert_eq!(pipe.server.stream_priority(12, 42, true), Ok(()));
        pipe.server.stream_send(12, b"b", false).unwrap();

        pipe.server.stream_recv(8, &mut b).unwrap();
        assert_eq!(pipe.server.stream_priority(8, 10, true), Ok(()));
        pipe.server.stream_send(8, b"b", false).unwrap();

        pipe.server.stream_recv(4, &mut b).unwrap();
        assert_eq!(pipe.server.stream_priority(4, 42, true), Ok(()));
        pipe.server.stream_send(4, b"b", false).unwrap();

        // Stream 0 is re-prioritized!!!
        assert_eq!(pipe.server.stream_priority(0, 20, true), Ok(()));

        // First is stream 8.
        let (len, _) = pipe.server.send(&mut buf).unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();

        assert_eq!(
            frames.first(),
            Some(&frame::Frame::Stream {
                stream_id: 8,
                data: stream::RangeBuf::from(b"b", 0, false),
            })
        );

        // Then is stream 0.
        let (len, _) = pipe.server.send(&mut buf).unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();

        assert_eq!(
            frames.first(),
            Some(&frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(b"b", 0, false),
            })
        );

        // Then are stream 12 and 4, with the same priority.
        let (len, _) = pipe.server.send(&mut buf).unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();

        assert_eq!(
            frames.first(),
            Some(&frame::Frame::Stream {
                stream_id: 12,
                data: stream::RangeBuf::from(b"b", 0, false),
            })
        );

        let (len, _) = pipe.server.send(&mut buf).unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();

        assert_eq!(
            frames.first(),
            Some(&frame::Frame::Stream {
                stream_id: 4,
                data: stream::RangeBuf::from(b"b", 0, false),
            })
        );

        assert_eq!(pipe.server.send(&mut buf), Err(Error::Done));
    }

    #[test]
    /// Tests that streams and datagrams are correctly scheduled.
    fn stream_datagram_priority() {
        // Limit 1-RTT packet size to avoid congestion control interference.
        const MAX_TEST_PACKET_SIZE: usize = 540;

        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(1_000_000);
        config.set_initial_max_stream_data_bidi_local(1_000_000);
        config.set_initial_max_stream_data_bidi_remote(1_000_000);
        config.set_initial_max_stream_data_uni(0);
        config.set_initial_max_streams_bidi(100);
        config.set_initial_max_streams_uni(0);
        config.enable_dgram(true, 10, 10);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.stream_send(0, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(4, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        let mut b = [0; 1];

        let out = [b'b'; 500];

        // Server prioritizes Stream 0 and 4 with the same urgency with
        // incremental, meaning the frames should be sent in round-robin
        // fashion. It also sends DATAGRAMS which are always interleaved with
        // STREAM frames. So we'll expect a mix of frame types regardless
        // of the order that the application writes things in.

        pipe.server.stream_recv(0, &mut b).unwrap();
        assert_eq!(pipe.server.stream_priority(0, 255, true), Ok(()));
        pipe.server.stream_send(0, &out, false).unwrap();
        pipe.server.stream_send(0, &out, false).unwrap();
        pipe.server.stream_send(0, &out, false).unwrap();

        assert_eq!(pipe.server.stream_priority(4, 255, true), Ok(()));
        pipe.server.stream_send(4, &out, false).unwrap();
        pipe.server.stream_send(4, &out, false).unwrap();
        pipe.server.stream_send(4, &out, false).unwrap();

        for _ in 1..=6 {
            assert_eq!(pipe.server.dgram_send(&out), Ok(()));
        }

        let mut off_0 = 0;
        let mut off_4 = 0;

        for _ in 1..=3 {
            // DATAGRAM
            let (len, _) =
                pipe.server.send(&mut buf[..MAX_TEST_PACKET_SIZE]).unwrap();

            let frames =
                testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
            let mut frame_iter = frames.iter();

            assert_eq!(frame_iter.next().unwrap(), &frame::Frame::Datagram {
                data: out.into()
            });
            assert_eq!(frame_iter.next(), None);

            // STREAM 0
            let (len, _) =
                pipe.server.send(&mut buf[..MAX_TEST_PACKET_SIZE]).unwrap();

            let frames =
                testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
            let mut frame_iter = frames.iter();
            let stream = frame_iter.next().unwrap();

            assert_eq!(stream, &frame::Frame::Stream {
                stream_id: 0,
                data: stream::RangeBuf::from(&out, off_0, false),
            });

            off_0 = match stream {
                frame::Frame::Stream { data, .. } => data.max_off(),

                _ => unreachable!(),
            };
            assert_eq!(frame_iter.next(), None);

            // DATAGRAM
            let (len, _) =
                pipe.server.send(&mut buf[..MAX_TEST_PACKET_SIZE]).unwrap();

            let frames =
                testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
            let mut frame_iter = frames.iter();

            assert_eq!(frame_iter.next().unwrap(), &frame::Frame::Datagram {
                data: out.into()
            });
            assert_eq!(frame_iter.next(), None);

            // STREAM 4
            let (len, _) =
                pipe.server.send(&mut buf[..MAX_TEST_PACKET_SIZE]).unwrap();

            let frames =
                testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
            let mut frame_iter = frames.iter();
            let stream = frame_iter.next().unwrap();

            assert_eq!(stream, &frame::Frame::Stream {
                stream_id: 4,
                data: stream::RangeBuf::from(&out, off_4, false),
            });

            off_4 = match stream {
                frame::Frame::Stream { data, .. } => data.max_off(),

                _ => unreachable!(),
            };
            assert_eq!(frame_iter.next(), None);
        }
    }

    #[test]
    /// Tests that old data is retransmitted on PTO.
    fn early_retransmit() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends stream data.
        assert_eq!(pipe.client.stream_send(0, b"a", false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        // Client sends more stream data, but packet is lost
        assert_eq!(pipe.client.stream_send(4, b"b", false), Ok(1));
        assert!(pipe.client.send(&mut buf).is_ok());

        // Wait until PTO expires. Since the RTT is very low, wait a bit more.
        let timer = pipe.client.timeout().unwrap();
        std::thread::sleep(timer + time::Duration::from_millis(1));

        pipe.client.on_timeout();

        let epoch = packet::Epoch::Application;
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .recovery
                .loss_probes(epoch),
            1,
        );

        // Client retransmits stream data in PTO probe.
        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .recovery
                .loss_probes(epoch),
            0,
        );

        let frames =
            testing::decode_pkt(&mut pipe.server, &mut buf[..len]).unwrap();

        let mut iter = frames.iter();

        // Skip ACK frame.
        iter.next();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::Stream {
                stream_id: 4,
                data: stream::RangeBuf::from(b"b", 0, false),
            })
        );
        assert_eq!(pipe.client.stats().retrans, 1);
    }

    #[test]
    /// Tests that PTO probe packets are not coalesced together.
    fn dont_coalesce_probes() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();

        // Client sends Initial packet.
        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(len, 1200);

        // Wait for PTO to expire.
        let timer = pipe.client.timeout().unwrap();
        std::thread::sleep(timer + time::Duration::from_millis(1));

        pipe.client.on_timeout();

        let epoch = packet::Epoch::Initial;
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .recovery
                .loss_probes(epoch),
            1,
        );

        // Client sends PTO probe.
        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(len, 1200);
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .recovery
                .loss_probes(epoch),
            0,
        );

        // Wait for PTO to expire.
        let timer = pipe.client.timeout().unwrap();
        std::thread::sleep(timer + time::Duration::from_millis(1));

        pipe.client.on_timeout();

        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .recovery
                .loss_probes(epoch),
            2,
        );

        // Client sends first PTO probe.
        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(len, 1200);
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .recovery
                .loss_probes(epoch),
            1,
        );

        // Client sends second PTO probe.
        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(len, 1200);
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .recovery
                .loss_probes(epoch),
            0,
        );
    }

    #[test]
    fn coalesce_padding_short() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();

        // Client sends first flight.
        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(len, MIN_CLIENT_INITIAL_LEN);
        assert_eq!(pipe.server_recv(&mut buf[..len]), Ok(len));

        // Server sends first flight.
        let (len, _) = pipe.server.send(&mut buf).unwrap();
        assert_eq!(len, MIN_CLIENT_INITIAL_LEN);
        assert_eq!(pipe.client_recv(&mut buf[..len]), Ok(len));

        let (len, _) = pipe.server.send(&mut buf).unwrap();
        assert_eq!(pipe.client_recv(&mut buf[..len]), Ok(len));

        // Client sends stream data.
        assert!(pipe.client.is_established());
        assert_eq!(pipe.client.stream_send(4, b"hello", true), Ok(5));

        // Client sends second flight.
        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(len, MIN_CLIENT_INITIAL_LEN);
        assert_eq!(pipe.server_recv(&mut buf[..len]), Ok(len));

        // None of the sent packets should have been dropped.
        assert_eq!(pipe.client.sent_count, pipe.server.recv_count);
        assert_eq!(pipe.server.sent_count, pipe.client.recv_count);
    }

    #[test]
    /// Tests that client avoids handshake deadlock by arming PTO.
    fn handshake_anti_deadlock() {
        let mut buf = [0; 65535];

        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert-big.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();

        let mut pipe = testing::Pipe::with_server_config(&mut config).unwrap();

        assert!(!pipe.client.handshake_status().has_handshake_keys);
        assert!(!pipe.client.handshake_status().peer_verified_address);
        assert!(!pipe.server.handshake_status().has_handshake_keys);
        assert!(pipe.server.handshake_status().peer_verified_address);

        // Client sends padded Initial.
        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(len, 1200);

        // Server receives client's Initial and sends own Initial and Handshake
        // until it's blocked by the anti-amplification limit.
        assert_eq!(pipe.server_recv(&mut buf[..len]), Ok(len));
        let flight = testing::emit_flight(&mut pipe.server).unwrap();

        assert!(!pipe.client.handshake_status().has_handshake_keys);
        assert!(!pipe.client.handshake_status().peer_verified_address);
        assert!(pipe.server.handshake_status().has_handshake_keys);
        assert!(pipe.server.handshake_status().peer_verified_address);

        // Client receives the server flight and sends Handshake ACK, but it is
        // lost.
        testing::process_flight(&mut pipe.client, flight).unwrap();
        testing::emit_flight(&mut pipe.client).unwrap();

        assert!(pipe.client.handshake_status().has_handshake_keys);
        assert!(!pipe.client.handshake_status().peer_verified_address);
        assert!(pipe.server.handshake_status().has_handshake_keys);
        assert!(pipe.server.handshake_status().peer_verified_address);

        // Make sure client's PTO timer is armed.
        assert!(pipe.client.timeout().is_some());
    }

    #[test]
    /// Tests that packets with corrupted type (from Handshake to Initial) are
    /// properly ignored.
    fn handshake_packet_type_corruption() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();

        // Client sends padded Initial.
        let (len, _) = pipe.client.send(&mut buf).unwrap();
        assert_eq!(len, 1200);

        // Server receives client's Initial and sends own Initial and Handshake.
        assert_eq!(pipe.server_recv(&mut buf[..len]), Ok(len));

        let flight = testing::emit_flight(&mut pipe.server).unwrap();
        testing::process_flight(&mut pipe.client, flight).unwrap();

        // Client sends Initial packet with ACK.
        let active_pid =
            pipe.client.paths.get_active_path_id().expect("no active");
        let (ty, len) = pipe
            .client
            .send_single(&mut buf, active_pid, false, time::Instant::now())
            .unwrap();
        assert_eq!(ty, Type::Initial);

        assert_eq!(pipe.server_recv(&mut buf[..len]), Ok(len));

        // Client sends Handshake packet.
        let (ty, len) = pipe
            .client
            .send_single(&mut buf, active_pid, false, time::Instant::now())
            .unwrap();
        assert_eq!(ty, Type::Handshake);

        // Packet type is corrupted to Initial.
        buf[0] &= !(0x20);

        let hdr = Header::from_slice(&mut buf[..len], 0).unwrap();
        assert_eq!(hdr.ty, Type::Initial);

        // Server receives corrupted packet without returning an error.
        assert_eq!(pipe.server_recv(&mut buf[..len]), Ok(len));
    }

    #[test]
    fn dgram_send_fails_invalidstate() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(
            pipe.client.dgram_send(b"hello, world"),
            Err(Error::InvalidState)
        );
    }

    #[test]
    fn dgram_send_app_limited() {
        let mut buf = [0; 65535];
        let send_buf = [0xcf; 1000];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(3);
        config.enable_dgram(true, 1000, 1000);
        config.set_max_recv_udp_payload_size(1200);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        for _ in 0..1000 {
            assert_eq!(pipe.client.dgram_send(&send_buf), Ok(()));
        }

        assert!(!pipe
            .client
            .paths
            .get_active()
            .expect("no active")
            .recovery
            .app_limited());
        assert_eq!(pipe.client.dgram_send_queue.byte_size(), 1_000_000);

        let (len, _) = pipe.client.send(&mut buf).unwrap();

        assert_ne!(pipe.client.dgram_send_queue.byte_size(), 0);
        assert_ne!(pipe.client.dgram_send_queue.byte_size(), 1_000_000);
        assert!(!pipe
            .client
            .paths
            .get_active()
            .expect("no active")
            .recovery
            .app_limited());

        assert_eq!(pipe.server_recv(&mut buf[..len]), Ok(len));

        let flight = testing::emit_flight(&mut pipe.client).unwrap();
        testing::process_flight(&mut pipe.server, flight).unwrap();

        let flight = testing::emit_flight(&mut pipe.server).unwrap();
        testing::process_flight(&mut pipe.client, flight).unwrap();

        assert_ne!(pipe.client.dgram_send_queue.byte_size(), 0);
        assert_ne!(pipe.client.dgram_send_queue.byte_size(), 1_000_000);

        assert!(!pipe
            .client
            .paths
            .get_active()
            .expect("no active")
            .recovery
            .app_limited());
    }

    #[test]
    fn dgram_single_datagram() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(3);
        config.enable_dgram(true, 10, 10);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.dgram_send(b"hello, world"), Ok(()));

        assert_eq!(pipe.advance(), Ok(()));

        let result1 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result1, Ok(12));

        let result2 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result2, Err(Error::Done));
    }

    #[test]
    fn dgram_multiple_datagrams() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(3);
        config.enable_dgram(true, 2, 3);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.dgram_send_queue_len(), 0);
        assert_eq!(pipe.client.dgram_send_queue_byte_size(), 0);

        assert_eq!(pipe.client.dgram_send(b"hello, world"), Ok(()));
        assert_eq!(pipe.client.dgram_send(b"ciao, mondo"), Ok(()));
        assert_eq!(pipe.client.dgram_send(b"hola, mundo"), Ok(()));
        assert!(pipe.client.is_dgram_send_queue_full());

        assert_eq!(pipe.client.dgram_send_queue_byte_size(), 34);

        pipe.client
            .dgram_purge_outgoing(|d: &[u8]| -> bool { d[0] == b'c' });

        assert_eq!(pipe.client.dgram_send_queue_len(), 2);
        assert_eq!(pipe.client.dgram_send_queue_byte_size(), 23);
        assert!(!pipe.client.is_dgram_send_queue_full());

        // Before packets exchanged, no dgrams on server receive side.
        assert_eq!(pipe.server.dgram_recv_queue_len(), 0);

        assert_eq!(pipe.advance(), Ok(()));

        // After packets exchanged, no dgrams on client send side.
        assert_eq!(pipe.client.dgram_send_queue_len(), 0);
        assert_eq!(pipe.client.dgram_send_queue_byte_size(), 0);

        assert_eq!(pipe.server.dgram_recv_queue_len(), 2);
        assert_eq!(pipe.server.dgram_recv_queue_byte_size(), 23);
        assert!(pipe.server.is_dgram_recv_queue_full());

        let result1 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result1, Ok(12));
        assert_eq!(buf[0], b'h');
        assert_eq!(buf[1], b'e');
        assert!(!pipe.server.is_dgram_recv_queue_full());

        let result2 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result2, Ok(11));
        assert_eq!(buf[0], b'h');
        assert_eq!(buf[1], b'o');

        let result3 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result3, Err(Error::Done));

        assert_eq!(pipe.server.dgram_recv_queue_len(), 0);
        assert_eq!(pipe.server.dgram_recv_queue_byte_size(), 0);
    }

    #[test]
    fn dgram_send_queue_overflow() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(3);
        config.enable_dgram(true, 10, 2);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.dgram_send(b"hello, world"), Ok(()));
        assert_eq!(pipe.client.dgram_send(b"ciao, mondo"), Ok(()));
        assert_eq!(pipe.client.dgram_send(b"hola, mundo"), Err(Error::Done));

        assert_eq!(pipe.advance(), Ok(()));

        let result1 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result1, Ok(12));
        assert_eq!(buf[0], b'h');
        assert_eq!(buf[1], b'e');

        let result2 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result2, Ok(11));
        assert_eq!(buf[0], b'c');
        assert_eq!(buf[1], b'i');

        let result3 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result3, Err(Error::Done));
    }

    #[test]
    fn dgram_recv_queue_overflow() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(3);
        config.enable_dgram(true, 2, 10);
        config.set_max_recv_udp_payload_size(1200);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.dgram_send(b"hello, world"), Ok(()));
        assert_eq!(pipe.client.dgram_send(b"ciao, mondo"), Ok(()));
        assert_eq!(pipe.client.dgram_send(b"hola, mundo"), Ok(()));

        assert_eq!(pipe.advance(), Ok(()));

        let result1 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result1, Ok(11));
        assert_eq!(buf[0], b'c');
        assert_eq!(buf[1], b'i');

        let result2 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result2, Ok(11));
        assert_eq!(buf[0], b'h');
        assert_eq!(buf[1], b'o');

        let result3 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result3, Err(Error::Done));
    }

    #[test]
    fn dgram_send_max_size() {
        let mut buf = [0; MAX_DGRAM_FRAME_SIZE as usize];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(3);
        config.enable_dgram(true, 10, 10);
        config.set_max_recv_udp_payload_size(1452);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();

        // Before handshake (before peer settings) we don't know max dgram size
        assert_eq!(pipe.client.dgram_max_writable_len(), None);

        assert_eq!(pipe.handshake(), Ok(()));

        let max_dgram_size = pipe.client.dgram_max_writable_len().unwrap();

        // Tests use a 16-byte connection ID, so the max datagram frame payload
        // size is (1200 byte-long packet - 40 bytes overhead)
        assert_eq!(max_dgram_size, 1160);

        let dgram_packet: Vec<u8> = vec![42; max_dgram_size];

        assert_eq!(pipe.client.dgram_send(&dgram_packet), Ok(()));

        assert_eq!(pipe.advance(), Ok(()));

        let result1 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result1, Ok(max_dgram_size));

        let result2 = pipe.server.dgram_recv(&mut buf);
        assert_eq!(result2, Err(Error::Done));
    }

    #[test]
    /// Tests is_readable check.
    fn is_readable() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);
        config.set_initial_max_streams_uni(3);
        config.enable_dgram(true, 10, 10);
        config.set_max_recv_udp_payload_size(1452);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // No readable data.
        assert!(!pipe.client.is_readable());
        assert!(!pipe.server.is_readable());

        assert_eq!(pipe.client.stream_send(4, b"aaaaa", false), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Server received stream.
        assert!(!pipe.client.is_readable());
        assert!(pipe.server.is_readable());

        assert_eq!(
            pipe.server.stream_send(4, b"aaaaaaaaaaaaaaa", false),
            Ok(15)
        );
        assert_eq!(pipe.advance(), Ok(()));

        // Client received stream.
        assert!(pipe.client.is_readable());
        assert!(pipe.server.is_readable());

        // Client drains stream.
        let mut b = [0; 15];
        pipe.client.stream_recv(4, &mut b).unwrap();
        assert_eq!(pipe.advance(), Ok(()));

        assert!(!pipe.client.is_readable());
        assert!(pipe.server.is_readable());

        // Server shuts down stream.
        assert_eq!(pipe.server.stream_shutdown(4, Shutdown::Read, 0), Ok(()));
        assert!(!pipe.server.is_readable());

        // Server received dgram.
        assert_eq!(pipe.client.dgram_send(b"dddddddddddddd"), Ok(()));
        assert_eq!(pipe.advance(), Ok(()));

        assert!(!pipe.client.is_readable());
        assert!(pipe.server.is_readable());

        // Client received dgram.
        assert_eq!(pipe.server.dgram_send(b"dddddddddddddd"), Ok(()));
        assert_eq!(pipe.advance(), Ok(()));

        assert!(pipe.client.is_readable());
        assert!(pipe.server.is_readable());

        // Drain the dgram queues.
        let r = pipe.server.dgram_recv(&mut buf);
        assert_eq!(r, Ok(14));
        assert!(!pipe.server.is_readable());

        let r = pipe.client.dgram_recv(&mut buf);
        assert_eq!(r, Ok(14));
        assert!(!pipe.client.is_readable());
    }

    #[test]
    fn close() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.close(false, 0x1234, b"hello?"), Ok(()));

        assert_eq!(
            pipe.client.close(false, 0x4321, b"hello?"),
            Err(Error::Done)
        );

        let (len, _) = pipe.client.send(&mut buf).unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.server, &mut buf[..len]).unwrap();

        assert_eq!(
            frames.first(),
            Some(&frame::Frame::ConnectionClose {
                error_code: 0x1234,
                frame_type: 0,
                reason: b"hello?".to_vec(),
            })
        );
    }

    #[test]
    fn app_close_by_client() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.close(true, 0x1234, b"hello!"), Ok(()));

        assert_eq!(pipe.client.close(true, 0x4321, b"hello!"), Err(Error::Done));

        let (len, _) = pipe.client.send(&mut buf).unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.server, &mut buf[..len]).unwrap();

        assert_eq!(
            frames.first(),
            Some(&frame::Frame::ApplicationClose {
                error_code: 0x1234,
                reason: b"hello!".to_vec(),
            })
        );
    }

    // OpenSSL does not provide a straightforward interface to deal with custom
    // off-load key signing.
    #[cfg(not(feature = "openssl"))]
    #[test]
    fn app_close_by_server_during_handshake_private_key_failure() {
        let mut pipe = testing::Pipe::new().unwrap();
        pipe.server.handshake.set_failing_private_key_method();

        // Client sends initial flight.
        let flight = testing::emit_flight(&mut pipe.client).unwrap();
        assert_eq!(
            testing::process_flight(&mut pipe.server, flight),
            Err(Error::TlsFail)
        );

        let flight = testing::emit_flight(&mut pipe.server).unwrap();

        // Both connections are not established.
        assert!(!pipe.server.is_established());
        assert!(!pipe.client.is_established());

        // Connection should already be closed due the failure during key signing.
        assert_eq!(
            pipe.server.close(true, 123, b"fail whale"),
            Err(Error::Done)
        );

        testing::process_flight(&mut pipe.client, flight).unwrap();

        // Connection should already be closed due the failure during key signing.
        assert_eq!(
            pipe.client.close(true, 123, b"fail whale"),
            Err(Error::Done)
        );

        // Connection is not established on the server / client (and never
        // will be)
        assert!(!pipe.server.is_established());
        assert!(!pipe.client.is_established());

        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(
            pipe.server.local_error(),
            Some(&ConnectionError {
                is_app: false,
                error_code: 0x01,
                reason: vec![],
            })
        );
        assert_eq!(
            pipe.client.peer_error(),
            Some(&ConnectionError {
                is_app: false,
                error_code: 0x01,
                reason: vec![],
            })
        );
    }

    #[test]
    fn app_close_by_server_during_handshake_not_established() {
        let mut pipe = testing::Pipe::new().unwrap();

        // Client sends initial flight.
        let flight = testing::emit_flight(&mut pipe.client).unwrap();
        testing::process_flight(&mut pipe.server, flight).unwrap();

        let flight = testing::emit_flight(&mut pipe.server).unwrap();

        // Both connections are not established.
        assert!(!pipe.client.is_established() && !pipe.server.is_established());

        // Server closes before connection is established.
        pipe.server.close(true, 123, b"fail whale").unwrap();

        testing::process_flight(&mut pipe.client, flight).unwrap();

        // Connection is established on the client.
        assert!(pipe.client.is_established());

        // Client sends after connection is established.
        pipe.client.stream_send(0, b"badauthtoken", true).unwrap();

        let flight = testing::emit_flight(&mut pipe.client).unwrap();
        testing::process_flight(&mut pipe.server, flight).unwrap();

        // Connection is not established on the server (and never will be)
        assert!(!pipe.server.is_established());

        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(
            pipe.server.local_error(),
            Some(&ConnectionError {
                is_app: false,
                error_code: 0x0c,
                reason: vec![],
            })
        );
        assert_eq!(
            pipe.client.peer_error(),
            Some(&ConnectionError {
                is_app: false,
                error_code: 0x0c,
                reason: vec![],
            })
        );
    }

    #[test]
    fn app_close_by_server_during_handshake_established() {
        let mut pipe = testing::Pipe::new().unwrap();

        // Client sends initial flight.
        let flight = testing::emit_flight(&mut pipe.client).unwrap();
        testing::process_flight(&mut pipe.server, flight).unwrap();

        let flight = testing::emit_flight(&mut pipe.server).unwrap();

        // Both connections are not established.
        assert!(!pipe.client.is_established() && !pipe.server.is_established());

        testing::process_flight(&mut pipe.client, flight).unwrap();

        // Connection is established on the client.
        assert!(pipe.client.is_established());

        // Client sends after connection is established.
        pipe.client.stream_send(0, b"badauthtoken", true).unwrap();

        let flight = testing::emit_flight(&mut pipe.client).unwrap();
        testing::process_flight(&mut pipe.server, flight).unwrap();

        // Connection is established on the server but the Handshake ACK has not
        // been sent yet.
        assert!(pipe.server.is_established());

        // Server closes after connection is established.
        pipe.server
            .close(true, 123, b"Invalid authentication")
            .unwrap();

        // Server sends Handshake ACK and then 1RTT CONNECTION_CLOSE.
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(
            pipe.server.local_error(),
            Some(&ConnectionError {
                is_app: true,
                error_code: 123,
                reason: b"Invalid authentication".to_vec()
            })
        );
        assert_eq!(
            pipe.client.peer_error(),
            Some(&ConnectionError {
                is_app: true,
                error_code: 123,
                reason: b"Invalid authentication".to_vec()
            })
        );
    }

    #[test]
    fn transport_close_by_client_during_handshake_established() {
        let mut pipe = testing::Pipe::new().unwrap();

        // Client sends initial flight.
        let flight = testing::emit_flight(&mut pipe.client).unwrap();
        testing::process_flight(&mut pipe.server, flight).unwrap();

        let flight = testing::emit_flight(&mut pipe.server).unwrap();

        // Both connections are not established.
        assert!(!pipe.client.is_established() && !pipe.server.is_established());

        testing::process_flight(&mut pipe.client, flight).unwrap();

        // Connection is established on the client.
        assert!(pipe.client.is_established());

        // Client sends after connection is established.
        pipe.client.close(false, 123, b"connection close").unwrap();

        let flight = testing::emit_flight(&mut pipe.client).unwrap();
        testing::process_flight(&mut pipe.server, flight).unwrap();

        assert_eq!(
            pipe.server.peer_error(),
            Some(&ConnectionError {
                is_app: false,
                error_code: 123,
                reason: b"connection close".to_vec()
            })
        );
        assert_eq!(
            pipe.client.local_error(),
            Some(&ConnectionError {
                is_app: false,
                error_code: 123,
                reason: b"connection close".to_vec()
            })
        );
    }

    #[test]
    fn peer_error() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.server.close(false, 0x1234, b"hello?"), Ok(()));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(
            pipe.client.peer_error(),
            Some(&ConnectionError {
                is_app: false,
                error_code: 0x1234u64,
                reason: b"hello?".to_vec()
            })
        );
    }

    #[test]
    fn app_peer_error() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.server.close(true, 0x1234, b"hello!"), Ok(()));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(
            pipe.client.peer_error(),
            Some(&ConnectionError {
                is_app: true,
                error_code: 0x1234u64,
                reason: b"hello!".to_vec()
            })
        );
    }

    #[test]
    fn local_error() {
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.server.local_error(), None);

        assert_eq!(pipe.server.close(true, 0x1234, b"hello!"), Ok(()));

        assert_eq!(
            pipe.server.local_error(),
            Some(&ConnectionError {
                is_app: true,
                error_code: 0x1234u64,
                reason: b"hello!".to_vec()
            })
        );
    }

    #[test]
    fn update_max_datagram_size() {
        let mut client_scid = [0; 16];
        rand::rand_bytes(&mut client_scid[..]);
        let client_scid = ConnectionId::from_ref(&client_scid);
        let client_addr = "127.0.0.1:1234".parse().unwrap();

        let mut server_scid = [0; 16];
        rand::rand_bytes(&mut server_scid[..]);
        let server_scid = ConnectionId::from_ref(&server_scid);
        let server_addr = "127.0.0.1:4321".parse().unwrap();

        let mut client_config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        client_config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        client_config.set_max_recv_udp_payload_size(1200);

        let mut server_config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        server_config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        server_config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        server_config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        server_config.verify_peer(false);
        server_config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        // Larger than the client
        server_config.set_max_send_udp_payload_size(1500);

        let mut pipe = testing::Pipe {
            client: connect(
                Some("quic.tech"),
                &client_scid,
                client_addr,
                server_addr,
                &mut client_config,
            )
            .unwrap(),
            server: accept(
                &server_scid,
                None,
                server_addr,
                client_addr,
                &mut server_config,
            )
            .unwrap(),
        };

        // Before handshake
        assert_eq!(
            pipe.server
                .paths
                .get_active()
                .expect("no active")
                .recovery
                .max_datagram_size(),
            1500,
        );

        assert_eq!(pipe.handshake(), Ok(()));

        // After handshake, max_datagram_size should match to client's
        // max_recv_udp_payload_size which is smaller
        assert_eq!(
            pipe.server
                .paths
                .get_active()
                .expect("no active")
                .recovery
                .max_datagram_size(),
            1200,
        );
        assert_eq!(
            pipe.server
                .paths
                .get_active()
                .expect("no active")
                .recovery
                .cwnd(),
            12000,
        );
    }

    #[test]
    /// Tests that connection-level send capacity decreases as more stream data
    /// is buffered.
    fn send_capacity() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(100000);
        config.set_initial_max_stream_data_bidi_local(10000);
        config.set_initial_max_stream_data_bidi_remote(10000);
        config.set_initial_max_streams_bidi(10);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.client.stream_send(0, b"hello!", true), Ok(6));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(4, b"hello!", true), Ok(6));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(8, b"hello!", true), Ok(6));
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.stream_send(12, b"hello!", true), Ok(6));
        assert_eq!(pipe.advance(), Ok(()));

        let mut r = pipe.server.readable().collect::<Vec<u64>>();
        assert_eq!(r.len(), 4);

        r.sort();

        assert_eq!(r, [0, 4, 8, 12]);

        assert_eq!(pipe.server.stream_recv(0, &mut buf), Ok((6, true)));
        assert_eq!(pipe.server.stream_recv(4, &mut buf), Ok((6, true)));
        assert_eq!(pipe.server.stream_recv(8, &mut buf), Ok((6, true)));
        assert_eq!(pipe.server.stream_recv(12, &mut buf), Ok((6, true)));

        assert_eq!(pipe.server.tx_cap, 12000);

        assert_eq!(pipe.server.stream_send(0, &buf[..5000], false), Ok(5000));
        assert_eq!(pipe.server.stream_send(4, &buf[..5000], false), Ok(5000));
        assert_eq!(pipe.server.stream_send(8, &buf[..5000], false), Ok(2000));

        // No more connection send capacity.
        assert_eq!(
            pipe.server.stream_send(12, &buf[..5000], false),
            Err(Error::Done)
        );
        assert_eq!(pipe.server.tx_cap, 0);

        assert_eq!(pipe.advance(), Ok(()));
    }

    #[cfg(feature = "boringssl-boring-crate")]
    #[test]
    fn user_provided_boring_ctx() -> Result<()> {
        // Manually construct boring ssl ctx for server
        let mut server_tls_ctx_builder =
            boring::ssl::SslContextBuilder::new(boring::ssl::SslMethod::tls())
                .unwrap();
        server_tls_ctx_builder
            .set_certificate_chain_file("examples/cert.crt")
            .unwrap();
        server_tls_ctx_builder
            .set_private_key_file(
                "examples/cert.key",
                boring::ssl::SslFiletype::PEM,
            )
            .unwrap();

        let mut server_config = Config::with_boring_ssl_ctx_builder(
            crate::PROTOCOL_VERSION,
            server_tls_ctx_builder,
        )?;
        let mut client_config = Config::new(crate::PROTOCOL_VERSION)?;
        client_config.load_cert_chain_from_pem_file("examples/cert.crt")?;
        client_config.load_priv_key_from_pem_file("examples/cert.key")?;

        for config in [&mut client_config, &mut server_config] {
            config.set_application_protos(&[b"proto1", b"proto2"])?;
            config.set_initial_max_data(30);
            config.set_initial_max_stream_data_bidi_local(15);
            config.set_initial_max_stream_data_bidi_remote(15);
            config.set_initial_max_stream_data_uni(10);
            config.set_initial_max_streams_bidi(3);
            config.set_initial_max_streams_uni(3);
            config.set_max_idle_timeout(180_000);
            config.verify_peer(false);
            config.set_ack_delay_exponent(8);
        }

        let mut client_scid = [0; 16];
        rand::rand_bytes(&mut client_scid[..]);
        let client_scid = ConnectionId::from_ref(&client_scid);
        let client_addr = "127.0.0.1:1234".parse().unwrap();

        let mut server_scid = [0; 16];
        rand::rand_bytes(&mut server_scid[..]);
        let server_scid = ConnectionId::from_ref(&server_scid);
        let server_addr = "127.0.0.1:4321".parse().unwrap();

        let mut pipe = testing::Pipe {
            client: connect(
                Some("quic.tech"),
                &client_scid,
                client_addr,
                server_addr,
                &mut client_config,
            )?,
            server: accept(
                &server_scid,
                None,
                server_addr,
                client_addr,
                &mut server_config,
            )?,
        };

        assert_eq!(pipe.handshake(), Ok(()));

        Ok(())
    }

    #[test]
    /// Tests that resetting a stream restores flow control for unsent data.
    fn last_tx_data_larger_than_tx_data() {
        let mut config = Config::new(PROTOCOL_VERSION).unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(12000);
        config.set_initial_max_stream_data_bidi_local(20000);
        config.set_initial_max_stream_data_bidi_remote(20000);
        config.set_max_recv_udp_payload_size(1200);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_client_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client opens stream 4 and 8.
        assert_eq!(pipe.client.stream_send(4, b"a", true), Ok(1));
        assert_eq!(pipe.client.stream_send(8, b"b", true), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        // Server reads stream data.
        let mut b = [0; 15];
        pipe.server.stream_recv(4, &mut b).unwrap();

        // Server sends stream data close to cwnd (12000).
        let buf = [0; 10000];
        assert_eq!(pipe.server.stream_send(4, &buf, false), Ok(10000));

        testing::emit_flight(&mut pipe.server).unwrap();

        // Server buffers some data, until send capacity limit reached.
        let mut buf = [0; 1200];
        assert_eq!(pipe.server.stream_send(4, &buf, false), Ok(1200));
        assert_eq!(pipe.server.stream_send(8, &buf, false), Ok(800));
        assert_eq!(pipe.server.stream_send(4, &buf, false), Err(Error::Done));

        // Wait for PTO to expire.
        let timer = pipe.server.timeout().unwrap();
        std::thread::sleep(timer + time::Duration::from_millis(1));

        pipe.server.on_timeout();

        // Server sends PTO probe (not limited to cwnd),
        // to update last_tx_data.
        let (len, _) = pipe.server.send(&mut buf).unwrap();
        assert_eq!(len, 1200);

        // Client sends STOP_SENDING to decrease tx_data
        // by unsent data. It will make last_tx_data > tx_data
        // and trigger #1232 bug.
        let frames = [frame::Frame::StopSending {
            stream_id: 4,
            error_code: 42,
        }];

        let pkt_type = packet::Type::Short;
        pipe.send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();
    }

    /// Tests that when the client provides a new ConnectionId, it eventually
    /// reaches the server and notifies the application.
    #[test]
    fn send_connection_ids() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(3);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // So far, there should not have any QUIC event.
        assert_eq!(pipe.client.path_event_next(), None);
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(pipe.client.scids_left(), 2);

        let (scid, reset_token) = testing::create_cid_and_reset_token(16);
        assert_eq!(pipe.client.new_scid(&scid, reset_token, false), Ok(1));

        // Let exchange packets over the connection.
        assert_eq!(pipe.advance(), Ok(()));

        // At this point, the server should be notified that it has a new CID.
        assert_eq!(pipe.server.available_dcids(), 1);
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(pipe.client.path_event_next(), None);
        assert_eq!(pipe.client.scids_left(), 1);

        // Now, a second CID can be provided.
        let (scid, reset_token) = testing::create_cid_and_reset_token(16);
        assert_eq!(pipe.client.new_scid(&scid, reset_token, false), Ok(2));

        // Let exchange packets over the connection.
        assert_eq!(pipe.advance(), Ok(()));

        // At this point, the server should be notified that it has a new CID.
        assert_eq!(pipe.server.available_dcids(), 2);
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(pipe.client.path_event_next(), None);
        assert_eq!(pipe.client.scids_left(), 0);

        // If now the client tries to send another CID, it reports an error
        // since it exceeds the limit of active CIDs.
        let (scid, reset_token) = testing::create_cid_and_reset_token(16);
        assert_eq!(
            pipe.client.new_scid(&scid, reset_token, false),
            Err(Error::IdLimit),
        );
    }

    #[test]
    /// Tests that NEW_CONNECTION_ID with zero-length CID are rejected.
    fn connection_id_zero() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let mut frames = Vec::new();

        // Client adds a CID that is too short.
        let (scid, reset_token) = testing::create_cid_and_reset_token(0);

        frames.push(frame::Frame::NewConnectionId {
            seq_num: 1,
            retire_prior_to: 0,
            conn_id: scid.to_vec(),
            reset_token: reset_token.to_be_bytes(),
        });

        let pkt_type = packet::Type::Short;

        let written =
            testing::encode_pkt(&mut pipe.client, pkt_type, &frames, &mut buf)
                .unwrap();

        let active_path = pipe.server.paths.get_active().unwrap();
        let info = RecvInfo {
            to: active_path.local_addr(),
            from: active_path.peer_addr(),
        };

        assert_eq!(
            pipe.server.recv(&mut buf[..written], info),
            Err(Error::InvalidFrame)
        );

        let written = match pipe.server.send(&mut buf) {
            Ok((write, _)) => write,

            Err(_) => unreachable!(),
        };

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..written]).unwrap();
        let mut iter = frames.iter();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::ConnectionClose {
                error_code: 0x7,
                frame_type: 0,
                reason: Vec::new(),
            })
        );
    }

    #[test]
    /// Tests that NEW_CONNECTION_ID with too long CID are rejected.
    fn connection_id_invalid_max_len() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let mut frames = Vec::new();

        // Client adds a CID that is too long.
        let (scid, reset_token) =
            testing::create_cid_and_reset_token(MAX_CONN_ID_LEN + 1);

        frames.push(frame::Frame::NewConnectionId {
            seq_num: 1,
            retire_prior_to: 0,
            conn_id: scid.to_vec(),
            reset_token: reset_token.to_be_bytes(),
        });

        let pkt_type = packet::Type::Short;

        let written =
            testing::encode_pkt(&mut pipe.client, pkt_type, &frames, &mut buf)
                .unwrap();

        let active_path = pipe.server.paths.get_active().unwrap();
        let info = RecvInfo {
            to: active_path.local_addr(),
            from: active_path.peer_addr(),
        };

        assert_eq!(
            pipe.server.recv(&mut buf[..written], info),
            Err(Error::InvalidFrame)
        );

        let written = match pipe.server.send(&mut buf) {
            Ok((write, _)) => write,

            Err(_) => unreachable!(),
        };

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..written]).unwrap();
        let mut iter = frames.iter();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::ConnectionClose {
                error_code: 0x7,
                frame_type: 0,
                reason: Vec::new(),
            })
        );
    }

    #[test]
    /// Exercises the handling of NEW_CONNECTION_ID and RETIRE_CONNECTION_ID
    /// frames.
    fn connection_id_handling() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // So far, there should not have any QUIC event.
        assert_eq!(pipe.client.path_event_next(), None);
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(pipe.client.scids_left(), 1);

        let scid = pipe.client.source_id().into_owned();

        let (scid_1, reset_token_1) = testing::create_cid_and_reset_token(16);
        assert_eq!(pipe.client.new_scid(&scid_1, reset_token_1, false), Ok(1));

        // Let exchange packets over the connection.
        assert_eq!(pipe.advance(), Ok(()));

        // At this point, the server should be notified that it has a new CID.
        assert_eq!(pipe.server.available_dcids(), 1);
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(pipe.client.path_event_next(), None);
        assert_eq!(pipe.client.scids_left(), 0);

        // Now we assume that the client wants to advertise more source
        // Connection IDs than the advertised limit. This is valid if it
        // requests its peer to retire enough Connection IDs to fit within the
        // limits.

        let (scid_2, reset_token_2) = testing::create_cid_and_reset_token(16);
        assert_eq!(pipe.client.new_scid(&scid_2, reset_token_2, true), Ok(2));

        // Let exchange packets over the connection.
        assert_eq!(pipe.advance(), Ok(()));

        // At this point, the server still have a spare DCID.
        assert_eq!(pipe.server.available_dcids(), 1);
        assert_eq!(pipe.server.path_event_next(), None);

        // Client should have received a retired notification.
        assert_eq!(pipe.client.retired_scid_next(), Some(scid));
        assert_eq!(pipe.client.retired_scid_next(), None);

        assert_eq!(pipe.client.path_event_next(), None);
        assert_eq!(pipe.client.scids_left(), 0);

        // The active Destination Connection ID of the server should now be the
        // one with sequence number 1.
        assert_eq!(pipe.server.destination_id(), scid_1);

        // Now tries to experience CID retirement. If the server tries to remove
        // non-existing DCIDs, it fails.
        assert_eq!(pipe.server.retire_dcid(0), Err(Error::InvalidState));
        assert_eq!(pipe.server.retire_dcid(3), Err(Error::InvalidState));

        // Now it removes DCID with sequence 1.
        assert_eq!(pipe.server.retire_dcid(1), Ok(()));

        // Let exchange packets over the connection.
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(pipe.client.retired_scid_next(), Some(scid_1));
        assert_eq!(pipe.client.retired_scid_next(), None);

        assert_eq!(pipe.server.destination_id(), scid_2);
        assert_eq!(pipe.server.available_dcids(), 0);

        // Trying to remove the last DCID triggers an error.
        assert_eq!(pipe.server.retire_dcid(2), Err(Error::OutOfIdentifiers));
    }

    #[test]
    fn lost_connection_id_frames() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let scid = pipe.client.source_id().into_owned();

        let (scid_1, reset_token_1) = testing::create_cid_and_reset_token(16);
        assert_eq!(pipe.client.new_scid(&scid_1, reset_token_1, false), Ok(1));

        // Packets are sent, but never received.
        testing::emit_flight(&mut pipe.client).unwrap();

        // Wait until timer expires. Since the RTT is very low, wait a bit more.
        let timer = pipe.client.timeout().unwrap();
        std::thread::sleep(timer + time::Duration::from_millis(1));

        pipe.client.on_timeout();

        // Let exchange packets over the connection.
        assert_eq!(pipe.advance(), Ok(()));

        // At this point, the server should be notified that it has a new CID.
        assert_eq!(pipe.server.available_dcids(), 1);

        // Now the server retires the first Destination CID.
        assert_eq!(pipe.server.retire_dcid(0), Ok(()));

        // But the packet never reaches the client.
        testing::emit_flight(&mut pipe.server).unwrap();

        // Wait until timer expires. Since the RTT is very low, wait a bit more.
        let timer = pipe.server.timeout().unwrap();
        std::thread::sleep(timer + time::Duration::from_millis(1));

        pipe.server.on_timeout();

        // Let exchange packets over the connection.
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.client.retired_scid_next(), Some(scid));
        assert_eq!(pipe.client.retired_scid_next(), None);
    }

    #[test]
    fn sending_duplicate_scids() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(3);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let (scid_1, reset_token_1) = testing::create_cid_and_reset_token(16);
        assert_eq!(pipe.client.new_scid(&scid_1, reset_token_1, false), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));

        // Trying to send the same CID with a different reset token raises an
        // InvalidState error.
        let reset_token_2 = reset_token_1.wrapping_add(1);
        assert_eq!(
            pipe.client.new_scid(&scid_1, reset_token_2, false),
            Err(Error::InvalidState),
        );

        // Retrying to send the exact same CID with the same token returns the
        // previously assigned CID seq, but without sending anything.
        assert_eq!(pipe.client.new_scid(&scid_1, reset_token_1, false), Ok(1));
        assert!(!pipe.client.ids.has_new_scids());

        // Now retire this new CID.
        assert_eq!(pipe.server.retire_dcid(1), Ok(()));
        assert_eq!(pipe.advance(), Ok(()));

        // It is up to the application to ensure that a given SCID is not reused
        // later.
        assert_eq!(pipe.client.new_scid(&scid_1, reset_token_1, false), Ok(2));
    }

    #[test]
    /// Tests the limit to retired DCID sequence numbers.
    fn connection_id_retire_limit() {
        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // So far, there should not have any QUIC event.
        assert_eq!(pipe.client.path_event_next(), None);
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(pipe.client.scids_left(), 1);

        let (scid_1, reset_token_1) = testing::create_cid_and_reset_token(16);
        assert_eq!(pipe.client.new_scid(&scid_1, reset_token_1, false), Ok(1));

        // Let exchange packets over the connection.
        assert_eq!(pipe.advance(), Ok(()));

        // At this point, the server should be notified that it has a new CID.
        assert_eq!(pipe.server.available_dcids(), 1);
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(pipe.client.path_event_next(), None);
        assert_eq!(pipe.client.scids_left(), 0);

        let mut frames = Vec::new();

        // Client retires more than 3x the number of allowed active CIDs.
        for i in 2..=7 {
            let (scid, reset_token) = testing::create_cid_and_reset_token(16);

            frames.push(frame::Frame::NewConnectionId {
                seq_num: i,
                retire_prior_to: i,
                conn_id: scid.to_vec(),
                reset_token: reset_token.to_be_bytes(),
            });
        }

        let pkt_type = packet::Type::Short;

        let written =
            testing::encode_pkt(&mut pipe.client, pkt_type, &frames, &mut buf)
                .unwrap();

        let active_path = pipe.server.paths.get_active().unwrap();
        let info = RecvInfo {
            to: active_path.local_addr(),
            from: active_path.peer_addr(),
        };

        assert_eq!(
            pipe.server.recv(&mut buf[..written], info),
            Err(Error::IdLimit)
        );

        let written = match pipe.server.send(&mut buf) {
            Ok((write, _)) => write,

            Err(_) => unreachable!(),
        };

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..written]).unwrap();
        let mut iter = frames.iter();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::ConnectionClose {
                error_code: 0x9,
                frame_type: 0,
                reason: Vec::new(),
            })
        );
    }

    // Utility function.
    fn pipe_with_exchanged_cids(
        config: &mut Config, client_scid_len: usize, server_scid_len: usize,
        additional_cids: usize,
    ) -> testing::Pipe {
        let mut pipe = testing::Pipe::with_config_and_scid_lengths(
            config,
            client_scid_len,
            server_scid_len,
        )
        .unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let mut c_cids = Vec::new();
        let mut c_reset_tokens = Vec::new();
        let mut s_cids = Vec::new();
        let mut s_reset_tokens = Vec::new();

        for i in 0..additional_cids {
            if client_scid_len > 0 {
                let (c_cid, c_reset_token) =
                    testing::create_cid_and_reset_token(client_scid_len);
                c_cids.push(c_cid);
                c_reset_tokens.push(c_reset_token);

                assert_eq!(
                    pipe.client.new_scid(&c_cids[i], c_reset_tokens[i], true),
                    Ok(i as u64 + 1)
                );
            }

            if server_scid_len > 0 {
                let (s_cid, s_reset_token) =
                    testing::create_cid_and_reset_token(server_scid_len);
                s_cids.push(s_cid);
                s_reset_tokens.push(s_reset_token);
                assert_eq!(
                    pipe.server.new_scid(&s_cids[i], s_reset_tokens[i], true),
                    Ok(i as u64 + 1)
                );
            }
        }

        // Let exchange packets over the connection.
        assert_eq!(pipe.advance(), Ok(()));

        if client_scid_len > 0 {
            assert_eq!(pipe.server.available_dcids(), additional_cids);
        }

        if server_scid_len > 0 {
            assert_eq!(pipe.client.available_dcids(), additional_cids);
        }

        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(pipe.client.path_event_next(), None);

        pipe
    }

    #[test]
    fn path_validation() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let server_addr = testing::Pipe::server_addr();
        let client_addr_2 = "127.0.0.1:5678".parse().unwrap();

        // We cannot probe a new path if there are not enough identifiers.
        assert_eq!(
            pipe.client.probe_path(client_addr_2, server_addr),
            Err(Error::OutOfIdentifiers)
        );

        let (c_cid, c_reset_token) = testing::create_cid_and_reset_token(16);

        assert_eq!(pipe.client.new_scid(&c_cid, c_reset_token, true), Ok(1));

        let (s_cid, s_reset_token) = testing::create_cid_and_reset_token(16);
        assert_eq!(pipe.server.new_scid(&s_cid, s_reset_token, true), Ok(1));

        // We need to exchange the CIDs first.
        assert_eq!(
            pipe.client.probe_path(client_addr_2, server_addr),
            Err(Error::OutOfIdentifiers)
        );

        // Let exchange packets over the connection.
        assert_eq!(pipe.advance(), Ok(()));

        assert_eq!(pipe.server.available_dcids(), 1);
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(pipe.client.available_dcids(), 1);
        assert_eq!(pipe.client.path_event_next(), None);

        // Now the path probing can work.
        assert_eq!(pipe.client.probe_path(client_addr_2, server_addr), Ok(1));

        // But the server cannot probe a yet-unseen path.
        assert_eq!(
            pipe.server.probe_path(server_addr, client_addr_2),
            Err(Error::InvalidState),
        );

        assert_eq!(pipe.advance(), Ok(()));

        // The path should be validated at some point.
        assert_eq!(
            pipe.client.path_event_next(),
            Some(PathEvent::Validated(client_addr_2, server_addr)),
        );
        assert_eq!(pipe.client.path_event_next(), None);

        // The server should be notified of this new path.
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::New(server_addr, client_addr_2)),
        );
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::Validated(server_addr, client_addr_2)),
        );
        assert_eq!(pipe.server.path_event_next(), None);

        // The server can later probe the path again.
        assert_eq!(pipe.server.probe_path(server_addr, client_addr_2), Ok(1));

        // This should not trigger any event at client side.
        assert_eq!(pipe.client.path_event_next(), None);
        assert_eq!(pipe.server.path_event_next(), None);
    }

    #[test]
    fn losing_probing_packets() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);

        let mut pipe = pipe_with_exchanged_cids(&mut config, 16, 16, 1);

        let server_addr = testing::Pipe::server_addr();
        let client_addr_2 = "127.0.0.1:5678".parse().unwrap();
        assert_eq!(pipe.client.probe_path(client_addr_2, server_addr), Ok(1));

        // The client creates the PATH CHALLENGE, but it is lost.
        testing::emit_flight(&mut pipe.client).unwrap();

        // Wait until probing timer expires. Since the RTT is very low,
        // wait a bit more.
        let probed_pid = pipe
            .client
            .paths
            .path_id_from_addrs(&(client_addr_2, server_addr))
            .unwrap();
        let probe_instant = pipe
            .client
            .paths
            .get(probed_pid)
            .unwrap()
            .recovery
            .loss_detection_timer()
            .unwrap();
        let timer = probe_instant.duration_since(time::Instant::now());
        std::thread::sleep(timer + time::Duration::from_millis(1));

        pipe.client.on_timeout();

        assert_eq!(pipe.advance(), Ok(()));

        // The path should be validated at some point.
        assert_eq!(
            pipe.client.path_event_next(),
            Some(PathEvent::Validated(client_addr_2, server_addr))
        );
        assert_eq!(pipe.client.path_event_next(), None);

        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::New(server_addr, client_addr_2))
        );
        // The path should be validated at some point.
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::Validated(server_addr, client_addr_2))
        );
        assert_eq!(pipe.server.path_event_next(), None);
    }

    #[test]
    fn failed_path_validation() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);

        let mut pipe = pipe_with_exchanged_cids(&mut config, 16, 16, 1);

        let server_addr = testing::Pipe::server_addr();
        let client_addr_2 = "127.0.0.1:5678".parse().unwrap();
        assert_eq!(pipe.client.probe_path(client_addr_2, server_addr), Ok(1));

        for _ in 0..MAX_PROBING_TIMEOUTS {
            // The client creates the PATH CHALLENGE, but it is always lost.
            testing::emit_flight(&mut pipe.client).unwrap();

            // Wait until probing timer expires. Since the RTT is very low,
            // wait a bit more.
            let probed_pid = pipe
                .client
                .paths
                .path_id_from_addrs(&(client_addr_2, server_addr))
                .unwrap();
            let probe_instant = pipe
                .client
                .paths
                .get(probed_pid)
                .unwrap()
                .recovery
                .loss_detection_timer()
                .unwrap();
            let timer = probe_instant.duration_since(time::Instant::now());
            std::thread::sleep(timer + time::Duration::from_millis(1));

            pipe.client.on_timeout();
        }

        assert_eq!(
            pipe.client.path_event_next(),
            Some(PathEvent::FailedValidation(client_addr_2, server_addr)),
        );
    }

    #[test]
    fn client_discard_unknown_address() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_uni(3);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Server sends stream data.
        assert_eq!(pipe.server.stream_send(3, b"a", true), Ok(1));

        let mut flight =
            testing::emit_flight(&mut pipe.server).expect("no packet");
        // Let's change the address info.
        flight
            .iter_mut()
            .for_each(|(_, si)| si.from = "127.0.0.1:9292".parse().unwrap());
        assert_eq!(testing::process_flight(&mut pipe.client, flight), Ok(()));
        assert_eq!(pipe.client.paths.len(), 1);
    }

    #[test]
    fn path_validation_limited_mtu() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);

        let mut pipe = pipe_with_exchanged_cids(&mut config, 16, 16, 1);

        let server_addr = testing::Pipe::server_addr();
        let client_addr_2 = "127.0.0.1:5678".parse().unwrap();
        assert_eq!(pipe.client.probe_path(client_addr_2, server_addr), Ok(1));
        // Limited MTU of 1199 bytes for some reason.
        testing::process_flight(
            &mut pipe.server,
            testing::emit_flight_with_max_buffer(
                &mut pipe.client,
                1199,
                None,
                None,
            )
            .expect("no packet"),
        )
        .expect("error when processing client packets");
        testing::process_flight(
            &mut pipe.client,
            testing::emit_flight(&mut pipe.server).expect("no packet"),
        )
        .expect("error when processing client packets");
        let probed_pid = pipe
            .client
            .paths
            .path_id_from_addrs(&(client_addr_2, server_addr))
            .unwrap();
        assert!(!pipe.client.paths.get(probed_pid).unwrap().validated(),);
        assert_eq!(pipe.client.path_event_next(), None);
        // Now let the client probe at its MTU.
        assert_eq!(pipe.advance(), Ok(()));
        assert!(pipe.client.paths.get(probed_pid).unwrap().validated());
        assert_eq!(
            pipe.client.path_event_next(),
            Some(PathEvent::Validated(client_addr_2, server_addr))
        );
    }

    #[test]
    fn path_probing_dos() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);

        let mut pipe = pipe_with_exchanged_cids(&mut config, 16, 16, 1);

        let server_addr = testing::Pipe::server_addr();
        let client_addr_2 = "127.0.0.1:5678".parse().unwrap();
        assert_eq!(pipe.client.probe_path(client_addr_2, server_addr), Ok(1));

        assert_eq!(pipe.advance(), Ok(()));

        // The path should be validated at some point.
        assert_eq!(
            pipe.client.path_event_next(),
            Some(PathEvent::Validated(client_addr_2, server_addr))
        );
        assert_eq!(pipe.client.path_event_next(), None);

        // The server should be notified of this new path.
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::New(server_addr, client_addr_2))
        );
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::Validated(server_addr, client_addr_2))
        );
        assert_eq!(pipe.server.path_event_next(), None);

        assert_eq!(pipe.server.paths.len(), 2);

        // Now forge a packet reusing the unverified path's CID over another
        // 4-tuple.
        assert_eq!(pipe.client.probe_path(client_addr_2, server_addr), Ok(1));
        let client_addr_3 = "127.0.0.1:9012".parse().unwrap();
        let mut flight =
            testing::emit_flight(&mut pipe.client).expect("no generated packet");
        flight
            .iter_mut()
            .for_each(|(_, si)| si.from = client_addr_3);
        testing::process_flight(&mut pipe.server, flight)
            .expect("failed to process");
        assert_eq!(pipe.server.paths.len(), 2);
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::ReusedSourceConnectionId(
                1,
                (server_addr, client_addr_2),
                (server_addr, client_addr_3)
            ))
        );
        assert_eq!(pipe.server.path_event_next(), None);
    }

    #[test]
    fn retiring_active_path_dcid() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);

        let mut pipe = pipe_with_exchanged_cids(&mut config, 16, 16, 1);
        let server_addr = testing::Pipe::server_addr();
        let client_addr_2 = "127.0.0.1:5678".parse().unwrap();
        assert_eq!(pipe.client.probe_path(client_addr_2, server_addr), Ok(1));

        assert_eq!(pipe.client.retire_dcid(0), Err(Error::OutOfIdentifiers));
    }

    #[test]
    fn send_on_path_test() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_initial_max_data(100000);
        config.set_initial_max_stream_data_bidi_local(100000);
        config.set_initial_max_stream_data_bidi_remote(100000);
        config.set_initial_max_streams_bidi(2);
        config.set_active_connection_id_limit(4);

        let mut pipe = pipe_with_exchanged_cids(&mut config, 16, 16, 3);

        let server_addr = testing::Pipe::server_addr();
        let client_addr = testing::Pipe::client_addr();
        let client_addr_2 = "127.0.0.1:5678".parse().unwrap();
        assert_eq!(pipe.client.probe_path(client_addr_2, server_addr), Ok(1));

        let mut buf = [0; 65535];
        // There is nothing to send on the initial path.
        assert_eq!(
            pipe.client.send_on_path(
                &mut buf,
                Some(client_addr),
                Some(server_addr)
            ),
            Err(Error::Done)
        );

        // Client should send padded PATH_CHALLENGE.
        let (sent, si) = pipe
            .client
            .send_on_path(&mut buf, Some(client_addr_2), Some(server_addr))
            .expect("No error");
        assert_eq!(sent, MIN_CLIENT_INITIAL_LEN);
        assert_eq!(si.from, client_addr_2);
        assert_eq!(si.to, server_addr);

        let ri = RecvInfo {
            to: si.to,
            from: si.from,
        };
        assert_eq!(pipe.server.recv(&mut buf[..sent], ri), Ok(sent));

        let stats = pipe.server.stats();
        assert_eq!(stats.path_challenge_rx_count, 1);

        // A non-existing 4-tuple raises an InvalidState.
        let client_addr_3 = "127.0.0.1:9012".parse().unwrap();
        let server_addr_2 = "127.0.0.1:9876".parse().unwrap();
        assert_eq!(
            pipe.client.send_on_path(
                &mut buf,
                Some(client_addr_3),
                Some(server_addr)
            ),
            Err(Error::InvalidState)
        );
        assert_eq!(
            pipe.client.send_on_path(
                &mut buf,
                Some(client_addr),
                Some(server_addr_2)
            ),
            Err(Error::InvalidState)
        );

        // Let's introduce some additional path challenges and data exchange.
        assert_eq!(pipe.client.probe_path(client_addr, server_addr_2), Ok(2));
        assert_eq!(pipe.client.probe_path(client_addr_3, server_addr), Ok(3));
        // Just to fit in two packets.
        assert_eq!(pipe.client.stream_send(0, &buf[..1201], true), Ok(1201));

        // PATH_CHALLENGE
        let (sent, si) = pipe
            .client
            .send_on_path(&mut buf, Some(client_addr), None)
            .expect("No error");
        assert_eq!(sent, MIN_CLIENT_INITIAL_LEN);
        assert_eq!(si.from, client_addr);
        assert_eq!(si.to, server_addr_2);

        let ri = RecvInfo {
            to: si.to,
            from: si.from,
        };
        assert_eq!(pipe.server.recv(&mut buf[..sent], ri), Ok(sent));

        let stats = pipe.server.stats();
        assert_eq!(stats.path_challenge_rx_count, 2);

        // STREAM frame on active path.
        let (sent, si) = pipe
            .client
            .send_on_path(&mut buf, Some(client_addr), None)
            .expect("No error");
        assert_eq!(si.from, client_addr);
        assert_eq!(si.to, server_addr);

        let ri = RecvInfo {
            to: si.to,
            from: si.from,
        };
        assert_eq!(pipe.server.recv(&mut buf[..sent], ri), Ok(sent));

        let stats = pipe.server.stats();
        assert_eq!(stats.path_challenge_rx_count, 2);

        // PATH_CHALLENGE
        let (sent, si) = pipe
            .client
            .send_on_path(&mut buf, None, Some(server_addr))
            .expect("No error");
        assert_eq!(sent, MIN_CLIENT_INITIAL_LEN);
        assert_eq!(si.from, client_addr_3);
        assert_eq!(si.to, server_addr);

        let ri = RecvInfo {
            to: si.to,
            from: si.from,
        };
        assert_eq!(pipe.server.recv(&mut buf[..sent], ri), Ok(sent));

        let stats = pipe.server.stats();
        assert_eq!(stats.path_challenge_rx_count, 3);

        // STREAM frame on active path.
        let (sent, si) = pipe
            .client
            .send_on_path(&mut buf, None, Some(server_addr))
            .expect("No error");
        assert_eq!(si.from, client_addr);
        assert_eq!(si.to, server_addr);

        let ri = RecvInfo {
            to: si.to,
            from: si.from,
        };
        assert_eq!(pipe.server.recv(&mut buf[..sent], ri), Ok(sent));

        // No more data to exchange leads to Error::Done.
        assert_eq!(
            pipe.client.send_on_path(&mut buf, Some(client_addr), None),
            Err(Error::Done)
        );
        assert_eq!(
            pipe.client.send_on_path(&mut buf, None, Some(server_addr)),
            Err(Error::Done)
        );

        assert_eq!(pipe.advance(), Ok(()));

        let mut v1 = pipe.client.paths_iter(client_addr).collect::<Vec<_>>();
        let mut v2 = vec![server_addr, server_addr_2];

        v1.sort();
        v2.sort();

        assert_eq!(v1, v2);

        let mut v1 = pipe.client.paths_iter(client_addr_2).collect::<Vec<_>>();
        let mut v2 = vec![server_addr];

        v1.sort();
        v2.sort();

        assert_eq!(v1, v2);

        let mut v1 = pipe.client.paths_iter(client_addr_3).collect::<Vec<_>>();
        let mut v2 = vec![server_addr];

        v1.sort();
        v2.sort();

        assert_eq!(v1, v2);

        let stats = pipe.server.stats();
        assert_eq!(stats.path_challenge_rx_count, 3);
    }

    #[test]
    fn connection_migration() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(3);
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);

        let mut pipe = pipe_with_exchanged_cids(&mut config, 16, 16, 2);

        let server_addr = testing::Pipe::server_addr();
        let client_addr_2 = "127.0.0.1:5678".parse().unwrap();
        let client_addr_3 = "127.0.0.1:9012".parse().unwrap();
        let client_addr_4 = "127.0.0.1:8908".parse().unwrap();

        // Case 1: the client first probes the new address, the server too, and
        // then migrates.
        assert_eq!(pipe.client.probe_path(client_addr_2, server_addr), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));
        assert_eq!(
            pipe.client.path_event_next(),
            Some(PathEvent::Validated(client_addr_2, server_addr))
        );
        assert_eq!(pipe.client.path_event_next(), None);
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::New(server_addr, client_addr_2))
        );
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::Validated(server_addr, client_addr_2))
        );
        assert_eq!(
            pipe.client.is_path_validated(client_addr_2, server_addr),
            Ok(true)
        );
        assert_eq!(
            pipe.server.is_path_validated(server_addr, client_addr_2),
            Ok(true)
        );
        // The server can never initiates the connection migration.
        assert_eq!(
            pipe.server.migrate(server_addr, client_addr_2),
            Err(Error::InvalidState)
        );
        assert_eq!(pipe.client.migrate(client_addr_2, server_addr), Ok(1));
        assert_eq!(pipe.client.stream_send(0, b"data", true), Ok(4));
        assert_eq!(pipe.advance(), Ok(()));
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .local_addr(),
            client_addr_2
        );
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .peer_addr(),
            server_addr
        );
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::PeerMigrated(server_addr, client_addr_2))
        );
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(
            pipe.server
                .paths
                .get_active()
                .expect("no active")
                .local_addr(),
            server_addr
        );
        assert_eq!(
            pipe.server
                .paths
                .get_active()
                .expect("no active")
                .peer_addr(),
            client_addr_2
        );

        // Case 2: the client migrates on a path that was not previously
        // validated, and has spare SCIDs/DCIDs to do so.
        assert_eq!(pipe.client.migrate(client_addr_3, server_addr), Ok(2));
        assert_eq!(pipe.client.stream_send(4, b"data", true), Ok(4));
        assert_eq!(pipe.advance(), Ok(()));
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .local_addr(),
            client_addr_3
        );
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .peer_addr(),
            server_addr
        );
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::New(server_addr, client_addr_3))
        );
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::Validated(server_addr, client_addr_3))
        );
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::PeerMigrated(server_addr, client_addr_3))
        );
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(
            pipe.server
                .paths
                .get_active()
                .expect("no active")
                .local_addr(),
            server_addr
        );
        assert_eq!(
            pipe.server
                .paths
                .get_active()
                .expect("no active")
                .peer_addr(),
            client_addr_3
        );

        // Case 3: the client tries to migrate on the current active path.
        // This is not an error, but it triggers nothing.
        assert_eq!(pipe.client.migrate(client_addr_3, server_addr), Ok(2));
        assert_eq!(pipe.client.stream_send(8, b"data", true), Ok(4));
        assert_eq!(pipe.advance(), Ok(()));
        assert_eq!(pipe.client.path_event_next(), None);
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .local_addr(),
            client_addr_3
        );
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .peer_addr(),
            server_addr
        );
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(
            pipe.server
                .paths
                .get_active()
                .expect("no active")
                .local_addr(),
            server_addr
        );
        assert_eq!(
            pipe.server
                .paths
                .get_active()
                .expect("no active")
                .peer_addr(),
            client_addr_3
        );

        // Case 4: the client tries to migrate on a path that was not previously
        // validated, and has no spare SCIDs/DCIDs. Prevent active migration.
        assert_eq!(
            pipe.client.migrate(client_addr_4, server_addr),
            Err(Error::OutOfIdentifiers)
        );
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .local_addr(),
            client_addr_3
        );
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .peer_addr(),
            server_addr
        );
    }

    #[test]
    fn connection_migration_zero_length_cid() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);

        let mut pipe = pipe_with_exchanged_cids(&mut config, 0, 16, 1);

        let server_addr = testing::Pipe::server_addr();
        let client_addr_2 = "127.0.0.1:5678".parse().unwrap();

        // The client migrates on a path that was not previously
        // validated, and has spare SCIDs/DCIDs to do so.
        assert_eq!(pipe.client.migrate(client_addr_2, server_addr), Ok(1));
        assert_eq!(pipe.client.stream_send(4, b"data", true), Ok(4));
        assert_eq!(pipe.advance(), Ok(()));
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .local_addr(),
            client_addr_2
        );
        assert_eq!(
            pipe.client
                .paths
                .get_active()
                .expect("no active")
                .peer_addr(),
            server_addr
        );
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::New(server_addr, client_addr_2))
        );
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::Validated(server_addr, client_addr_2))
        );
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::PeerMigrated(server_addr, client_addr_2))
        );
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(
            pipe.server
                .paths
                .get_active()
                .expect("no active")
                .local_addr(),
            server_addr
        );
        assert_eq!(
            pipe.server
                .paths
                .get_active()
                .expect("no active")
                .peer_addr(),
            client_addr_2
        );
    }

    #[test]
    fn connection_migration_reordered_non_probing() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(2);
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);

        let mut pipe = pipe_with_exchanged_cids(&mut config, 16, 16, 1);

        let client_addr = testing::Pipe::client_addr();
        let server_addr = testing::Pipe::server_addr();
        let client_addr_2 = "127.0.0.1:5678".parse().unwrap();

        assert_eq!(pipe.client.probe_path(client_addr_2, server_addr), Ok(1));
        assert_eq!(pipe.advance(), Ok(()));
        assert_eq!(
            pipe.client.path_event_next(),
            Some(PathEvent::Validated(client_addr_2, server_addr))
        );
        assert_eq!(pipe.client.path_event_next(), None);
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::New(server_addr, client_addr_2))
        );
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::Validated(server_addr, client_addr_2))
        );
        assert_eq!(pipe.server.path_event_next(), None);

        // A first flight sent from secondary address.
        assert_eq!(pipe.client.stream_send(0, b"data", true), Ok(4));
        let mut first = testing::emit_flight(&mut pipe.client).unwrap();
        first.iter_mut().for_each(|(_, si)| si.from = client_addr_2);
        // A second one, but sent from the original one.
        assert_eq!(pipe.client.stream_send(4, b"data", true), Ok(4));
        let second = testing::emit_flight(&mut pipe.client).unwrap();
        // Second flight is received before first one.
        assert_eq!(testing::process_flight(&mut pipe.server, second), Ok(()));
        assert_eq!(testing::process_flight(&mut pipe.server, first), Ok(()));

        // Server does not perform connection migration because of packet
        // reordering.
        assert_eq!(pipe.server.path_event_next(), None);
        assert_eq!(
            pipe.server
                .paths
                .get_active()
                .expect("no active")
                .peer_addr(),
            client_addr
        );
    }

    #[test]
    fn resilience_against_migration_attack() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(3);
        config.set_initial_max_data(100000);
        config.set_initial_max_stream_data_bidi_local(100000);
        config.set_initial_max_stream_data_bidi_remote(100000);
        config.set_initial_max_streams_bidi(2);

        let mut pipe = pipe_with_exchanged_cids(&mut config, 16, 16, 1);

        let client_addr = testing::Pipe::client_addr();
        let server_addr = testing::Pipe::server_addr();
        let spoofed_client_addr = "127.0.0.1:6666".parse().unwrap();

        const DATA_BYTES: usize = 24000;
        let buf = [42; DATA_BYTES];
        let mut recv_buf = [0; DATA_BYTES];
        assert_eq!(pipe.server.stream_send(1, &buf, true), Ok(12000));
        assert_eq!(
            testing::process_flight(
                &mut pipe.client,
                testing::emit_flight(&mut pipe.server).unwrap()
            ),
            Ok(())
        );
        let (rcv_data_1, _) = pipe.client.stream_recv(1, &mut recv_buf).unwrap();

        // Fake the source address of client.
        let mut faked_addr_flight =
            testing::emit_flight(&mut pipe.client).unwrap();
        faked_addr_flight
            .iter_mut()
            .for_each(|(_, si)| si.from = spoofed_client_addr);
        assert_eq!(
            testing::process_flight(&mut pipe.server, faked_addr_flight),
            Ok(())
        );
        assert_eq!(pipe.server.stream_send(1, &buf[12000..], true), Ok(12000));
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::ReusedSourceConnectionId(
                0,
                (server_addr, client_addr),
                (server_addr, spoofed_client_addr)
            ))
        );
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::New(server_addr, spoofed_client_addr))
        );

        assert_eq!(
            pipe.server.is_path_validated(server_addr, client_addr),
            Ok(true)
        );
        assert_eq!(
            pipe.server
                .is_path_validated(server_addr, spoofed_client_addr),
            Ok(false)
        );

        // The client creates the PATH CHALLENGE, but it is always lost.
        testing::emit_flight(&mut pipe.server).unwrap();

        // Wait until probing timer expires. Since the RTT is very low,
        // wait a bit more.
        let probed_pid = pipe
            .server
            .paths
            .path_id_from_addrs(&(server_addr, spoofed_client_addr))
            .unwrap();
        let probe_instant = pipe
            .server
            .paths
            .get(probed_pid)
            .unwrap()
            .recovery
            .loss_detection_timer()
            .unwrap();
        let timer = probe_instant.duration_since(time::Instant::now());
        std::thread::sleep(timer + time::Duration::from_millis(1));

        pipe.server.on_timeout();

        // Because of the small ACK size, the server cannot send more to the
        // client. Fallback on the previous active path.
        assert_eq!(
            pipe.server.path_event_next(),
            Some(PathEvent::FailedValidation(
                server_addr,
                spoofed_client_addr
            ))
        );

        assert_eq!(
            pipe.server.is_path_validated(server_addr, client_addr),
            Ok(true)
        );
        assert_eq!(
            pipe.server
                .is_path_validated(server_addr, spoofed_client_addr),
            Ok(false)
        );

        let server_active_path = pipe.server.paths.get_active().unwrap();
        assert_eq!(server_active_path.local_addr(), server_addr);
        assert_eq!(server_active_path.peer_addr(), client_addr);
        assert_eq!(pipe.advance(), Ok(()));
        let (rcv_data_2, fin) =
            pipe.client.stream_recv(1, &mut recv_buf).unwrap();
        assert!(fin);
        assert_eq!(rcv_data_1 + rcv_data_2, DATA_BYTES);
    }

    #[test]
    fn consecutive_non_ack_eliciting() {
        let mut buf = [0; 65535];

        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Client sends a bunch of PING frames, causing server to ACK (ACKs aren't
        // ack-eliciting)
        let frames = [frame::Frame::Ping { mtu_probe: None }];
        let pkt_type = packet::Type::Short;
        for _ in 0..24 {
            let len = pipe
                .send_pkt_to_server(pkt_type, &frames, &mut buf)
                .unwrap();
            assert!(len > 0);

            let frames =
                testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
            assert!(
                frames
                    .iter()
                    .all(|frame| matches!(frame, frame::Frame::ACK { .. })),
                "ACK only"
            );
        }

        // After 24 non-ack-eliciting, an ACK is explicitly elicited with a PING
        let len = pipe
            .send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();
        assert!(len > 0);

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
        assert!(
            frames
                .iter()
                .any(|frame| matches!(frame, frame::Frame::Ping {
                    mtu_probe: None
                })),
            "found a PING"
        );
    }

    #[test]
    fn send_ack_eliciting_causes_ping() {
        // First establish a connection
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Queue a PING frame
        pipe.server.send_ack_eliciting().unwrap();

        // Make sure ping is sent
        let mut buf = [0; 1500];
        let (len, _) = pipe.server.send(&mut buf).unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
        let mut iter = frames.iter();

        assert_eq!(iter.next(), Some(&frame::Frame::Ping { mtu_probe: None }));
    }

    #[test]
    fn send_ack_eliciting_no_ping() {
        // First establish a connection
        let mut pipe = testing::Pipe::new().unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Queue a PING frame
        pipe.server.send_ack_eliciting().unwrap();

        // Send a stream frame, which is ACK-eliciting to make sure the ping is
        // not sent
        assert_eq!(pipe.server.stream_send(1, b"a", false), Ok(1));

        // Make sure ping is not sent
        let mut buf = [0; 1500];
        let (len, _) = pipe.server.send(&mut buf).unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();
        let mut iter = frames.iter();

        assert!(matches!(
            iter.next(),
            Some(&frame::Frame::Stream {
                stream_id: 1,
                data: _
            })
        ));
        assert!(iter.next().is_none());
    }

    /// Tests that streams do not keep being "writable" after being collected
    /// on reset.
    #[test]
    fn stop_sending_stream_send_after_reset_stream_ack() {
        let mut b = [0; 15];

        let mut buf = [0; 65535];

        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.set_initial_max_data(999999999);
        config.set_initial_max_stream_data_bidi_local(30);
        config.set_initial_max_stream_data_bidi_remote(30);
        config.set_initial_max_stream_data_uni(30);
        config.set_initial_max_streams_bidi(1000);
        config.set_initial_max_streams_uni(0);
        config.verify_peer(false);

        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        assert_eq!(pipe.server.streams.len(), 0);
        assert_eq!(pipe.server.readable().len(), 0);
        assert_eq!(pipe.server.writable().len(), 0);

        // Client opens a load of streams
        assert_eq!(pipe.client.stream_send(0, b"hello", true), Ok(5));
        assert_eq!(pipe.client.stream_send(4, b"hello", true), Ok(5));
        assert_eq!(pipe.client.stream_send(8, b"hello", true), Ok(5));
        assert_eq!(pipe.client.stream_send(12, b"hello", true), Ok(5));
        assert_eq!(pipe.client.stream_send(16, b"hello", true), Ok(5));
        assert_eq!(pipe.client.stream_send(20, b"hello", true), Ok(5));
        assert_eq!(pipe.client.stream_send(24, b"hello", true), Ok(5));
        assert_eq!(pipe.client.stream_send(28, b"hello", true), Ok(5));
        assert_eq!(pipe.client.stream_send(32, b"hello", true), Ok(5));
        assert_eq!(pipe.client.stream_send(36, b"hello", true), Ok(5));
        assert_eq!(pipe.advance(), Ok(()));

        // Server iterators are populated
        let mut r = pipe.server.readable();
        assert_eq!(r.len(), 10);
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), Some(4));
        assert_eq!(r.next(), Some(8));
        assert_eq!(r.next(), Some(12));
        assert_eq!(r.next(), Some(16));
        assert_eq!(r.next(), Some(20));
        assert_eq!(r.next(), Some(24));
        assert_eq!(r.next(), Some(28));
        assert_eq!(r.next(), Some(32));
        assert_eq!(r.next(), Some(36));

        assert_eq!(r.next(), None);

        let mut w = pipe.server.writable();
        assert_eq!(w.len(), 10);
        assert_eq!(w.next(), Some(0));
        assert_eq!(w.next(), Some(4));
        assert_eq!(w.next(), Some(8));
        assert_eq!(w.next(), Some(12));
        assert_eq!(w.next(), Some(16));
        assert_eq!(w.next(), Some(20));
        assert_eq!(w.next(), Some(24));
        assert_eq!(w.next(), Some(28));
        assert_eq!(w.next(), Some(32));
        assert_eq!(w.next(), Some(36));
        assert_eq!(w.next(), None);

        // Read one stream
        assert_eq!(pipe.server.stream_recv(0, &mut b), Ok((5, true)));
        assert!(pipe.server.stream_finished(0));

        assert_eq!(pipe.server.readable().len(), 9);
        assert_eq!(pipe.server.writable().len(), 10);

        assert_eq!(pipe.server.stream_writable(0, 0), Ok(true));

        // Server sends data on stream 0, until blocked.
        while pipe.server.stream_send(0, b"world", false) != Err(Error::Done) {
            assert_eq!(pipe.advance(), Ok(()));
        }

        assert_eq!(pipe.server.writable().len(), 9);
        assert_eq!(pipe.server.stream_writable(0, 0), Ok(true));

        // Client sends STOP_SENDING.
        let frames = [frame::Frame::StopSending {
            stream_id: 0,
            error_code: 42,
        }];

        let pkt_type = packet::Type::Short;
        let len = pipe
            .send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();

        // Server sent a RESET_STREAM frame in response.
        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();

        let mut iter = frames.iter();

        // Skip ACK frame.
        iter.next();

        assert_eq!(
            iter.next(),
            Some(&frame::Frame::ResetStream {
                stream_id: 0,
                error_code: 42,
                final_size: 30,
            })
        );

        // Stream 0 is now writable in order to make apps aware of STOP_SENDING
        // via returning an error.
        let mut w = pipe.server.writable();
        assert_eq!(w.len(), 10);

        assert!(w.any(|s| s == 0));
        assert_eq!(
            pipe.server.stream_writable(0, 1),
            Err(Error::StreamStopped(42))
        );

        assert_eq!(pipe.server.writable().len(), 10);
        assert_eq!(pipe.server.streams.len(), 10);

        // Client acks RESET_STREAM frame.
        let mut ranges = ranges::RangeSet::default();
        ranges.insert(0..12);

        let frames = [frame::Frame::ACK {
            ack_delay: 15,
            ranges,
            ecn_counts: None,
        }];

        assert_eq!(pipe.send_pkt_to_server(pkt_type, &frames, &mut buf), Ok(0));

        // Stream is collected on the server after RESET_STREAM is acked.
        assert_eq!(pipe.server.streams.len(), 9);

        // Sending STOP_SENDING again shouldn't trigger RESET_STREAM again.
        let frames = [frame::Frame::StopSending {
            stream_id: 0,
            error_code: 42,
        }];

        let len = pipe
            .send_pkt_to_server(pkt_type, &frames, &mut buf)
            .unwrap();

        let frames =
            testing::decode_pkt(&mut pipe.client, &mut buf[..len]).unwrap();

        assert_eq!(frames.len(), 1);

        match frames.first() {
            Some(frame::Frame::ACK { .. }) => (),

            f => panic!("expected ACK frame, got {:?}", f),
        };

        assert_eq!(pipe.server.streams.len(), 9);

        // Stream 0 has been collected and must not be writable anymore.
        let mut w = pipe.server.writable();
        assert_eq!(w.len(), 9);
        assert!(!w.any(|s| s == 0));

        // If we called send before the client ACK of reset stream, it would
        // have failed with StreamStopped.
        assert_eq!(pipe.server.stream_send(0, b"world", true), Err(Error::Done),);

        // Stream 0 is still not writable.
        let mut w = pipe.server.writable();
        assert_eq!(w.len(), 9);
        assert!(!w.any(|s| s == 0));
    }

    #[test]
    fn challenge_no_cids() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_active_connection_id_limit(4);
        config.set_initial_max_data(30);
        config.set_initial_max_stream_data_bidi_local(15);
        config.set_initial_max_stream_data_bidi_remote(15);
        config.set_initial_max_stream_data_uni(10);
        config.set_initial_max_streams_bidi(3);

        let mut pipe =
            testing::Pipe::with_config_and_scid_lengths(&mut config, 16, 16)
                .unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        // Server send CIDs to client
        let mut server_cids = Vec::new();
        for _ in 0..2 {
            let (cid, reset_token) = testing::create_cid_and_reset_token(16);
            pipe.server
                .new_scid(&cid, reset_token, true)
                .expect("server issue cid");
            server_cids.push(cid);
        }
        assert_eq!(pipe.advance(), Ok(()));

        let server_addr = testing::Pipe::server_addr();
        let client_addr_2 = "127.0.0.1:5678".parse().unwrap();

        // Client probes path before sending CIDs (simulating race condition)
        let frames = [frame::Frame::PathChallenge {
            data: [0, 1, 2, 3, 4, 5, 6, 7],
        }];
        let mut pkt_buf = [0u8; 1500];
        let mut b = octets::OctetsMut::with_slice(&mut pkt_buf);
        let epoch = packet::Type::Short.to_epoch().unwrap();
        let space = &mut pipe.client.pkt_num_spaces[epoch];
        let pn = space.next_pkt_num;
        let pn_len = 4;

        let hdr = Header {
            ty: packet::Type::Short,
            version: pipe.client.version,
            dcid: server_cids[0].clone(),
            scid: ConnectionId::from_ref(&[5, 4, 3, 2, 1]),
            pkt_num: 0,
            pkt_num_len: pn_len,
            token: pipe.client.token.clone(),
            versions: None,
            key_phase: pipe.client.key_phase,
        };
        hdr.to_bytes(&mut b).expect("encode header");
        let payload_len = frames.iter().fold(0, |acc, x| acc + x.wire_len());
        b.put_u32(pn as u32).expect("put pn");

        let payload_offset = b.off();

        for frame in frames {
            frame.to_bytes(&mut b).expect("encode frames");
        }

        let aead = space.crypto_seal.as_ref().expect("crypto seal");

        let written = packet::encrypt_pkt(
            &mut b,
            pn,
            pn_len,
            payload_len,
            payload_offset,
            None,
            aead,
        )
        .expect("packet encrypt");
        space.next_pkt_num += 1;

        pipe.server
            .recv(&mut pkt_buf[..written], RecvInfo {
                to: server_addr,
                from: client_addr_2,
            })
            .expect("server receive path challenge");

        // Show that the new path is not considered a destination path by quiche
        assert!(!pipe
            .server
            .paths_iter(server_addr)
            .any(|path| path == client_addr_2));
    }

    #[test]
    fn successful_probe_pmtud() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_initial_max_data(100000);
        config.set_initial_max_stream_data_bidi_local(100000);
        config.set_initial_max_stream_data_bidi_remote(100000);
        config.set_initial_max_streams_bidi(2);
        config.set_active_connection_id_limit(4);
        config.set_max_send_udp_payload_size(1350);
        config.set_max_recv_udp_payload_size(1350);
        config.discover_pmtu(true);

        // Perform initial handshake.
        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let server_addr = testing::Pipe::server_addr();
        let client_addr = testing::Pipe::client_addr();
        let pid_1 = pipe
            .server
            .paths
            .path_id_from_addrs(&(server_addr, client_addr))
            .expect("no such path");

        // Check that PMTU params are configured correctly
        let pmtu_param = &mut pipe.server.paths.get_mut(pid_1).unwrap().pmtud;
        assert!(pmtu_param.get_probe_status());
        assert_eq!(pmtu_param.get_probe_size(), 1350);
        assert_eq!(pipe.advance(), Ok(()));

        for (_, p) in pipe.server.paths.iter_mut() {
            assert_eq!(p.pmtud.get_current(), 1350);
            assert!(!p.pmtud.get_probe_status());
        }
    }

    #[test]
    fn pmtud_probe_loss() {
        let mut config = Config::new(crate::PROTOCOL_VERSION).unwrap();
        config
            .load_cert_chain_from_pem_file("examples/cert.crt")
            .unwrap();
        config
            .load_priv_key_from_pem_file("examples/cert.key")
            .unwrap();
        config
            .set_application_protos(&[b"proto1", b"proto2"])
            .unwrap();
        config.verify_peer(false);
        config.set_initial_max_data(100000);
        config.set_initial_max_stream_data_bidi_local(100000);
        config.set_initial_max_stream_data_bidi_remote(100000);
        config.set_initial_max_streams_bidi(2);
        config.set_active_connection_id_limit(4);
        config.set_max_send_udp_payload_size(1350);
        config.set_max_recv_udp_payload_size(1250);
        config.discover_pmtu(true);

        // Perform initial handshake.
        let mut pipe = testing::Pipe::with_config(&mut config).unwrap();
        assert_eq!(pipe.handshake(), Ok(()));

        let server_addr = testing::Pipe::server_addr();
        let client_addr = testing::Pipe::client_addr();
        let pid_1 = pipe
            .server
            .paths
            .path_id_from_addrs(&(server_addr, client_addr))
            .expect("no such path");

        // Check that PMTU params are configured correctly
        let pmtu_param = &mut pipe.server.paths.get_mut(pid_1).unwrap().pmtud;
        assert!(pmtu_param.get_probe_status());
        assert_eq!(pmtu_param.get_probe_size(), 1350);
        std::thread::sleep(
            pipe.server.paths.get_mut(pid_1).unwrap().recovery.rtt() +
                time::Duration::from_millis(1),
        );

        let active_server_path = pipe.server.paths.get_active_mut().unwrap();
        let pmtu_param = &mut active_server_path.pmtud;

        // PMTU not updated since probe is not ACKed
        assert_eq!(pmtu_param.get_current(), 1200);

        // Continue searching for PMTU
        assert!(pmtu_param.get_probe_status());
    }
}

pub use crate::packet::ConnectionId;
pub use crate::packet::Header;
pub use crate::packet::Type;

pub use crate::path::PathEvent;
pub use crate::path::PathStats;
pub use crate::path::SocketAddrIter;

pub use crate::recovery::congestion::CongestionControlAlgorithm;

pub use crate::stream::StreamIter;

mod cid;
mod crypto;
mod dgram;
#[cfg(feature = "ffi")]
mod ffi;
mod flowcontrol;
mod frame;
pub mod h3;
mod minmax;
pub mod oack;
mod packet;
mod path;
mod pmtud;
mod rand;
mod ranges;
mod recovery;
mod stream;
mod tls;
