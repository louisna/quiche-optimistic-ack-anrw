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

use std::ffi;
use std::ptr;
use std::slice;

use std::io::Write;

use once_cell::sync::Lazy;

use libc::c_char;
use libc::c_int;
use libc::c_uint;
use libc::c_void;

use crate::Error;
use crate::Result;

use crate::Connection;
use crate::ConnectionError;

use crate::crypto;
use crate::packet;

const TLS1_3_VERSION: u16 = 0x0304;
const TLS_ALERT_ERROR: u64 = 0x100;
const INTERNAL_ERROR: u64 = 0x01;

#[allow(non_camel_case_types)]
#[repr(transparent)]
struct SSL_METHOD {
    _unused: c_void,
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
struct SSL_CTX {
    _unused: c_void,
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
struct SSL {
    _unused: c_void,
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
struct SSL_CIPHER {
    _unused: c_void,
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
struct SSL_SESSION {
    _unused: c_void,
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
struct X509_VERIFY_PARAM {
    _unused: c_void,
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
#[cfg(windows)]
struct X509_STORE {
    _unused: c_void,
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
struct X509_STORE_CTX {
    _unused: c_void,
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
#[cfg(windows)]
struct X509 {
    _unused: c_void,
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
struct STACK_OF {
    _unused: c_void,
}

#[cfg(test)]
#[repr(C)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
enum ssl_private_key_result_t {
    ssl_private_key_success,
    ssl_private_key_retry,
    ssl_private_key_failure,
}

/// BoringSSL ex_data index for quiche connections.
///
/// TODO: replace with `std::sync::LazyLock` when stable.
pub static QUICHE_EX_DATA_INDEX: Lazy<c_int> = Lazy::new(|| unsafe {
    SSL_get_ex_new_index(0, ptr::null(), ptr::null(), ptr::null(), ptr::null())
});

pub struct Context(*mut SSL_CTX);

impl Context {
    // Note: some vendor-specific methods are implemented by each vendor's
    // submodule (openssl-quictls / boringssl).
    pub fn new() -> Result<Context> {
        unsafe {
            let ctx_raw = SSL_CTX_new(TLS_method());

            let mut ctx = Context(ctx_raw);

            ctx.set_session_callback();

            ctx.load_ca_certs()?;

            Ok(ctx)
        }
    }

    #[cfg(feature = "boringssl-boring-crate")]
    pub fn from_boring(
        ssl_ctx_builder: boring::ssl::SslContextBuilder,
    ) -> Context {
        use foreign_types_shared::ForeignType;

        let mut ctx = Context(ssl_ctx_builder.build().into_ptr() as _);
        ctx.set_session_callback();

        ctx
    }

    pub fn new_handshake(&mut self) -> Result<Handshake> {
        unsafe {
            let ssl = SSL_new(self.as_mut_ptr());
            Ok(Handshake::new(ssl))
        }
    }

    pub fn load_verify_locations_from_file(&mut self, file: &str) -> Result<()> {
        let file = ffi::CString::new(file).map_err(|_| Error::TlsFail)?;
        map_result(unsafe {
            SSL_CTX_load_verify_locations(
                self.as_mut_ptr(),
                file.as_ptr(),
                std::ptr::null(),
            )
        })
    }

    pub fn load_verify_locations_from_directory(
        &mut self, path: &str,
    ) -> Result<()> {
        let path = ffi::CString::new(path).map_err(|_| Error::TlsFail)?;
        map_result(unsafe {
            SSL_CTX_load_verify_locations(
                self.as_mut_ptr(),
                std::ptr::null(),
                path.as_ptr(),
            )
        })
    }

    pub fn use_certificate_chain_file(&mut self, file: &str) -> Result<()> {
        let cstr = ffi::CString::new(file).map_err(|_| Error::TlsFail)?;
        map_result(unsafe {
            SSL_CTX_use_certificate_chain_file(self.as_mut_ptr(), cstr.as_ptr())
        })
    }

    pub fn use_privkey_file(&mut self, file: &str) -> Result<()> {
        let cstr = ffi::CString::new(file).map_err(|_| Error::TlsFail)?;
        map_result(unsafe {
            SSL_CTX_use_PrivateKey_file(self.as_mut_ptr(), cstr.as_ptr(), 1)
        })
    }

    #[cfg(not(windows))]
    fn load_ca_certs(&mut self) -> Result<()> {
        unsafe { map_result(SSL_CTX_set_default_verify_paths(self.as_mut_ptr())) }
    }

    #[cfg(windows)]
    fn load_ca_certs(&mut self) -> Result<()> {
        unsafe {
            let cstr = ffi::CString::new("Root").map_err(|_| Error::TlsFail)?;
            let sys_store = winapi::um::wincrypt::CertOpenSystemStoreA(
                0,
                cstr.as_ptr() as winapi::um::winnt::LPCSTR,
            );
            if sys_store.is_null() {
                return Err(Error::TlsFail);
            }

            let ctx_store = SSL_CTX_get_cert_store(self.as_mut_ptr());
            if ctx_store.is_null() {
                return Err(Error::TlsFail);
            }

            let mut ctx_p = winapi::um::wincrypt::CertEnumCertificatesInStore(
                sys_store,
                ptr::null(),
            );

            while !ctx_p.is_null() {
                let in_p = (*ctx_p).pbCertEncoded as *const u8;

                let cert = d2i_X509(
                    ptr::null_mut(),
                    &in_p,
                    (*ctx_p).cbCertEncoded as i32,
                );
                if !cert.is_null() {
                    X509_STORE_add_cert(ctx_store, cert);
                }

                X509_free(cert);

                ctx_p = winapi::um::wincrypt::CertEnumCertificatesInStore(
                    sys_store, ctx_p,
                );
            }

            // tidy up
            winapi::um::wincrypt::CertFreeCertificateContext(ctx_p);
            winapi::um::wincrypt::CertCloseStore(sys_store, 0);
        }

        Ok(())
    }

    fn set_session_callback(&mut self) {
        unsafe {
            // This is needed to enable the session callback on the client. On
            // the server it doesn't do anything.
            SSL_CTX_set_session_cache_mode(
                self.as_mut_ptr(),
                0x0001, // SSL_SESS_CACHE_CLIENT
            );

            SSL_CTX_sess_set_new_cb(self.as_mut_ptr(), Some(new_session));
        };
    }

    pub fn set_verify(&mut self, verify: bool) {
        // true  -> 0x01 SSL_VERIFY_PEER
        // false -> 0x00 SSL_VERIFY_NONE
        let mode = i32::from(verify);

        // Note: Base on two used modes(see above), it seems ok for both, bssl and
        // ossl. If mode needs to be ored then it may need to be adjusted.
        unsafe {
            SSL_CTX_set_verify(self.as_mut_ptr(), mode, None);
        }
    }

    pub fn enable_keylog(&mut self) {
        unsafe {
            SSL_CTX_set_keylog_callback(self.as_mut_ptr(), Some(keylog));
        }
    }

    pub fn set_alpn(&mut self, v: &[&[u8]]) -> Result<()> {
        let mut protos: Vec<u8> = Vec::new();

        for proto in v {
            protos.push(proto.len() as u8);
            protos.extend_from_slice(proto);
        }

        // Configure ALPN for servers.
        unsafe {
            SSL_CTX_set_alpn_select_cb(
                self.as_mut_ptr(),
                Some(select_alpn),
                ptr::null_mut(),
            );
        }

        // Configure ALPN for clients.
        map_result_zero_is_success(unsafe {
            SSL_CTX_set_alpn_protos(
                self.as_mut_ptr(),
                protos.as_ptr(),
                protos.len(),
            )
        })
    }

    pub fn set_ticket_key(&mut self, key: &[u8]) -> Result<()> {
        map_result(unsafe {
            SSL_CTX_set_tlsext_ticket_keys(
                self.as_mut_ptr(),
                key.as_ptr(),
                key.len(),
            )
        })
    }

    fn as_mut_ptr(&mut self) -> *mut SSL_CTX {
        self.0
    }
}

// NOTE: These traits are not automatically implemented for Context due to the
// raw pointer it wraps. However, the underlying data is not aliased (as Context
// should be its only owner), and there is no interior mutability, as the
// pointer is not accessed directly outside of this module, and the Context
// object API should preserve Rust's borrowing guarantees.
unsafe impl std::marker::Send for Context {}
unsafe impl std::marker::Sync for Context {}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { SSL_CTX_free(self.as_mut_ptr()) }
    }
}

pub struct Handshake {
    /// Raw pointer
    ptr: *mut SSL,
    /// SSL_process_quic_post_handshake should be called when whenever
    /// SSL_provide_quic_data is called to process the provided data.
    provided_data_outstanding: bool,
}

impl Handshake {
    // Note: some vendor-specific methods are implemented by each vendor's
    // submodule (openssl-quictls / boringssl).
    #[cfg(feature = "ffi")]
    pub unsafe fn from_ptr(ssl: *mut c_void) -> Handshake {
        Handshake::new(ssl as *mut SSL)
    }

    fn new(ptr: *mut SSL) -> Handshake {
        Handshake {
            ptr,
            provided_data_outstanding: false,
        }
    }

    pub fn get_error(&self, ret_code: c_int) -> c_int {
        unsafe { SSL_get_error(self.as_ptr(), ret_code) }
    }

    pub fn init(&mut self, is_server: bool) -> Result<()> {
        self.set_state(is_server);

        self.set_min_proto_version(TLS1_3_VERSION)?;
        self.set_max_proto_version(TLS1_3_VERSION)?;

        self.set_quic_method()?;

        // TODO: the early data context should include transport parameters and
        // HTTP/3 SETTINGS in wire format.
        self.set_quic_early_data_context(b"quiche")?;

        self.set_quiet_shutdown(true);

        Ok(())
    }

    pub fn use_legacy_codepoint(&mut self, use_legacy: bool) {
        unsafe {
            SSL_set_quic_use_legacy_codepoint(
                self.as_mut_ptr(),
                use_legacy as c_int,
            );
        }
    }

    pub fn set_state(&mut self, is_server: bool) {
        unsafe {
            if is_server {
                SSL_set_accept_state(self.as_mut_ptr());
            } else {
                SSL_set_connect_state(self.as_mut_ptr());
            }
        }
    }

    pub fn set_ex_data<T>(&mut self, idx: c_int, data: *const T) -> Result<()> {
        map_result(unsafe {
            let ptr = data as *mut c_void;
            SSL_set_ex_data(self.as_mut_ptr(), idx, ptr)
        })
    }

    pub fn set_quic_method(&mut self) -> Result<()> {
        map_result(unsafe {
            SSL_set_quic_method(self.as_mut_ptr(), &QUICHE_STREAM_METHOD)
        })
    }

    pub fn set_min_proto_version(&mut self, version: u16) -> Result<()> {
        map_result(unsafe {
            SSL_set_min_proto_version(self.as_mut_ptr(), version)
        })
    }

    pub fn set_max_proto_version(&mut self, version: u16) -> Result<()> {
        map_result(unsafe {
            SSL_set_max_proto_version(self.as_mut_ptr(), version)
        })
    }

    pub fn set_quiet_shutdown(&mut self, mode: bool) {
        unsafe { SSL_set_quiet_shutdown(self.as_mut_ptr(), i32::from(mode)) }
    }

    pub fn set_host_name(&mut self, name: &str) -> Result<()> {
        let cstr = ffi::CString::new(name).map_err(|_| Error::TlsFail)?;
        let rc =
            unsafe { SSL_set_tlsext_host_name(self.as_mut_ptr(), cstr.as_ptr()) };
        self.map_result_ssl(rc)?;

        let param = unsafe { SSL_get0_param(self.as_mut_ptr()) };

        map_result(unsafe {
            X509_VERIFY_PARAM_set1_host(param, cstr.as_ptr(), name.len())
        })
    }

    pub fn set_quic_transport_params(&mut self, buf: &[u8]) -> Result<()> {
        let rc = unsafe {
            SSL_set_quic_transport_params(
                self.as_mut_ptr(),
                buf.as_ptr(),
                buf.len(),
            )
        };
        self.map_result_ssl(rc)
    }

    pub fn quic_transport_params(&self) -> &[u8] {
        let mut ptr: *const u8 = ptr::null();
        let mut len: usize = 0;

        unsafe {
            SSL_get_peer_quic_transport_params(self.as_ptr(), &mut ptr, &mut len);
        }

        if len == 0 {
            return &mut [];
        }

        unsafe { slice::from_raw_parts(ptr, len) }
    }

    pub fn alpn_protocol(&self) -> &[u8] {
        let mut ptr: *const u8 = ptr::null();
        let mut len: u32 = 0;

        unsafe {
            SSL_get0_alpn_selected(self.as_ptr(), &mut ptr, &mut len);
        }

        if len == 0 {
            return &mut [];
        }

        unsafe { slice::from_raw_parts(ptr, len as usize) }
    }

    pub fn server_name(&self) -> Option<&str> {
        let s = unsafe {
            let ptr = SSL_get_servername(
                self.as_ptr(),
                0, // TLSEXT_NAMETYPE_host_name
            );

            if ptr.is_null() {
                return None;
            }

            ffi::CStr::from_ptr(ptr)
        };

        s.to_str().ok()
    }

    pub fn provide_data(
        &mut self, level: crypto::Level, buf: &[u8],
    ) -> Result<()> {
        self.provided_data_outstanding = true;
        let rc = unsafe {
            SSL_provide_quic_data(
                self.as_mut_ptr(),
                level,
                buf.as_ptr(),
                buf.len(),
            )
        };
        self.map_result_ssl(rc)
    }

    pub fn do_handshake(&mut self, ex_data: &mut ExData) -> Result<()> {
        self.set_ex_data(*QUICHE_EX_DATA_INDEX, ex_data)?;
        let rc = unsafe { SSL_do_handshake(self.as_mut_ptr()) };
        self.set_ex_data::<Connection>(*QUICHE_EX_DATA_INDEX, std::ptr::null())?;

        self.set_transport_error(ex_data, rc);
        self.map_result_ssl(rc)
    }

    pub fn process_post_handshake(&mut self, ex_data: &mut ExData) -> Result<()> {
        // If SSL_provide_quic_data hasn't been called since we last called
        // SSL_process_quic_post_handshake, then there's nothing to do.
        if !self.provided_data_outstanding {
            return Ok(());
        }
        self.provided_data_outstanding = false;

        self.set_ex_data(*QUICHE_EX_DATA_INDEX, ex_data)?;
        let rc = unsafe { SSL_process_quic_post_handshake(self.as_mut_ptr()) };
        self.set_ex_data::<Connection>(*QUICHE_EX_DATA_INDEX, std::ptr::null())?;

        self.set_transport_error(ex_data, rc);
        self.map_result_ssl(rc)
    }

    pub fn write_level(&self) -> crypto::Level {
        unsafe { SSL_quic_write_level(self.as_ptr()) }
    }

    pub fn cipher(&self) -> Option<crypto::Algorithm> {
        let cipher =
            map_result_ptr(unsafe { SSL_get_current_cipher(self.as_ptr()) });

        get_cipher_from_ptr(cipher.ok()?).ok()
    }

    #[cfg(test)]
    pub fn set_options(&mut self, opts: u32) {
        unsafe {
            SSL_set_options(self.as_mut_ptr(), opts);
        }
    }

    pub fn is_completed(&self) -> bool {
        unsafe { SSL_in_init(self.as_ptr()) == 0 }
    }

    pub fn is_resumed(&self) -> bool {
        unsafe { SSL_session_reused(self.as_ptr()) == 1 }
    }

    pub fn clear(&mut self) -> Result<()> {
        let rc = unsafe { SSL_clear(self.as_mut_ptr()) };
        self.map_result_ssl(rc)
    }

    fn as_ptr(&self) -> *const SSL {
        self.ptr
    }

    fn as_mut_ptr(&mut self) -> *mut SSL {
        self.ptr
    }

    fn map_result_ssl(&mut self, bssl_result: c_int) -> Result<()> {
        match bssl_result {
            1 => Ok(()),

            _ => {
                let ssl_err = self.get_error(bssl_result);
                info!("MAP SSL RESULT: {ssl_err}");
                match ssl_err {
                    // SSL_ERROR_SSL
                    1 => {
                        log_ssl_error();

                        Err(Error::TlsFail)
                    },

                    // SSL_ERROR_WANT_READ
                    2 => Err(Error::Done),

                    // SSL_ERROR_WANT_WRITE
                    3 => Err(Error::Done),

                    // SSL_ERROR_WANT_X509_LOOKUP
                    4 => Err(Error::Done),

                    // SSL_ERROR_SYSCALL
                    5 => Err(Error::TlsFail),

                    // SSL_ERROR_PENDING_SESSION
                    11 => Err(Error::Done),

                    // SSL_ERROR_PENDING_CERTIFICATE
                    12 => Err(Error::Done),

                    // SSL_ERROR_WANT_PRIVATE_KEY_OPERATION
                    13 => Err(Error::Done),

                    // SSL_ERROR_PENDING_TICKET
                    14 => Err(Error::Done),

                    // SSL_ERROR_EARLY_DATA_REJECTED
                    15 => {
                        self.reset_early_data_reject();
                        Err(Error::Done)
                    },

                    // SSL_ERROR_WANT_CERTIFICATE_VERIFY
                    16 => Err(Error::Done),

                    _ => Err(Error::TlsFail),
                }
            },
        }
    }

    fn set_transport_error(&mut self, ex_data: &mut ExData, bssl_result: c_int) {
        // SSL_ERROR_SSL
        if self.get_error(bssl_result) == 1 {
            // SSL_ERROR_SSL can't be recovered so ensure we set a
            // local_error so the connection is closed.
            // See https://www.openssl.org/docs/man1.1.1/man3/SSL_get_error.html
            if ex_data.local_error.is_none() {
                *ex_data.local_error = Some(ConnectionError {
                    is_app: false,
                    error_code: INTERNAL_ERROR,
                    reason: Vec::new(),
                })
            }
        }
    }

    #[cfg(feature = "boringssl-boring-crate")]
    pub(crate) fn ssl_mut(&mut self) -> &mut boring::ssl::SslRef {
        use foreign_types_shared::ForeignTypeRef;

        unsafe { boring::ssl::SslRef::from_ptr_mut(self.as_mut_ptr() as _) }
    }
}

// NOTE: These traits are not automatically implemented for Handshake due to the
// raw pointer it wraps. However, the underlying data is not aliased (as
// Handshake should be its only owner), and there is no interior mutability, as
// the pointer is not accessed directly outside of this module, and the
// Handshake object API should preserve Rust's borrowing guarantees.
unsafe impl std::marker::Send for Handshake {}
unsafe impl std::marker::Sync for Handshake {}

impl Drop for Handshake {
    fn drop(&mut self) {
        unsafe { SSL_free(self.as_mut_ptr()) }
    }
}

pub struct ExData<'a> {
    pub application_protos: &'a Vec<Vec<u8>>,

    pub pkt_num_spaces: &'a mut [packet::PktNumSpace; packet::Epoch::count()],

    pub session: &'a mut Option<Vec<u8>>,

    pub local_error: &'a mut Option<super::ConnectionError>,

    pub keylog: Option<&'a mut Box<dyn std::io::Write + Send + Sync>>,

    pub trace_id: &'a str,

    pub is_server: bool,
}

fn get_ex_data_from_ptr<'a, T>(ptr: *const SSL, idx: c_int) -> Option<&'a mut T> {
    unsafe {
        let data = SSL_get_ex_data(ptr, idx) as *mut T;
        data.as_mut()
    }
}

fn get_cipher_from_ptr(cipher: *const SSL_CIPHER) -> Result<crypto::Algorithm> {
    let cipher_id = unsafe { SSL_CIPHER_get_id(cipher) };

    let alg = match cipher_id {
        0x0300_1301 => crypto::Algorithm::AES128_GCM,
        0x0300_1302 => crypto::Algorithm::AES256_GCM,
        0x0300_1303 => crypto::Algorithm::ChaCha20_Poly1305,
        _ => return Err(Error::TlsFail),
    };

    Ok(alg)
}

extern fn set_read_secret(
    ssl: *mut SSL, level: crypto::Level, cipher: *const SSL_CIPHER,
    secret: *const u8, secret_len: usize,
) -> c_int {
    let ex_data = match get_ex_data_from_ptr::<ExData>(ssl, *QUICHE_EX_DATA_INDEX)
    {
        Some(v) => v,

        None => return 0,
    };

    trace!("{} set read secret lvl={:?}", ex_data.trace_id, level);

    let space = match level {
        crypto::Level::Initial =>
            &mut ex_data.pkt_num_spaces[packet::Epoch::Initial],
        crypto::Level::ZeroRTT =>
            &mut ex_data.pkt_num_spaces[packet::Epoch::Application],
        crypto::Level::Handshake =>
            &mut ex_data.pkt_num_spaces[packet::Epoch::Handshake],
        crypto::Level::OneRTT =>
            &mut ex_data.pkt_num_spaces[packet::Epoch::Application],
    };

    let aead = match get_cipher_from_ptr(cipher) {
        Ok(v) => v,

        Err(_) => return 0,
    };

    // 0-RTT read secrets are present only on the server.
    if level != crypto::Level::ZeroRTT || ex_data.is_server {
        let secret = unsafe { slice::from_raw_parts(secret, secret_len) };

        let open = match crypto::Open::from_secret(aead, secret) {
            Ok(v) => v,

            Err(_) => return 0,
        };

        if level == crypto::Level::ZeroRTT {
            space.crypto_0rtt_open = Some(open);
            return 1;
        }

        space.crypto_open = Some(open);
    }

    1
}

extern fn set_write_secret(
    ssl: *mut SSL, level: crypto::Level, cipher: *const SSL_CIPHER,
    secret: *const u8, secret_len: usize,
) -> c_int {
    let ex_data = match get_ex_data_from_ptr::<ExData>(ssl, *QUICHE_EX_DATA_INDEX)
    {
        Some(v) => v,

        None => return 0,
    };

    trace!("{} set write secret lvl={:?}", ex_data.trace_id, level);

    let space = match level {
        crypto::Level::Initial =>
            &mut ex_data.pkt_num_spaces[packet::Epoch::Initial],
        crypto::Level::ZeroRTT =>
            &mut ex_data.pkt_num_spaces[packet::Epoch::Application],
        crypto::Level::Handshake =>
            &mut ex_data.pkt_num_spaces[packet::Epoch::Handshake],
        crypto::Level::OneRTT =>
            &mut ex_data.pkt_num_spaces[packet::Epoch::Application],
    };

    let aead = match get_cipher_from_ptr(cipher) {
        Ok(v) => v,

        Err(_) => return 0,
    };

    // 0-RTT write secrets are present only on the client.
    if level != crypto::Level::ZeroRTT || !ex_data.is_server {
        let secret = unsafe { slice::from_raw_parts(secret, secret_len) };

        let seal = match crypto::Seal::from_secret(aead, secret) {
            Ok(v) => v,

            Err(_) => return 0,
        };

        space.crypto_seal = Some(seal);
    }

    1
}

extern fn add_handshake_data(
    ssl: *mut SSL, level: crypto::Level, data: *const u8, len: usize,
) -> c_int {
    let ex_data = match get_ex_data_from_ptr::<ExData>(ssl, *QUICHE_EX_DATA_INDEX)
    {
        Some(v) => v,

        None => return 0,
    };

    trace!(
        "{} write message lvl={:?} len={}",
        ex_data.trace_id,
        level,
        len
    );

    let buf = unsafe { slice::from_raw_parts(data, len) };

    let space = match level {
        crypto::Level::Initial =>
            &mut ex_data.pkt_num_spaces[packet::Epoch::Initial],
        crypto::Level::ZeroRTT => unreachable!(),
        crypto::Level::Handshake =>
            &mut ex_data.pkt_num_spaces[packet::Epoch::Handshake],
        crypto::Level::OneRTT =>
            &mut ex_data.pkt_num_spaces[packet::Epoch::Application],
    };

    if space.crypto_stream.send.write(buf, false).is_err() {
        return 0;
    }

    1
}

extern fn flush_flight(_ssl: *mut SSL) -> c_int {
    // We don't really need to anything here since the output packets are
    // generated separately, when conn.send() is called.

    1
}

extern fn send_alert(ssl: *mut SSL, level: crypto::Level, alert: u8) -> c_int {
    let ex_data = match get_ex_data_from_ptr::<ExData>(ssl, *QUICHE_EX_DATA_INDEX)
    {
        Some(v) => v,

        None => return 0,
    };

    trace!(
        "{} send alert lvl={:?} alert={:x}",
        ex_data.trace_id,
        level,
        alert
    );

    let error: u64 = TLS_ALERT_ERROR + u64::from(alert);
    *ex_data.local_error = Some(ConnectionError {
        is_app: false,
        error_code: error,
        reason: Vec::new(),
    });

    1
}

extern fn keylog(ssl: *const SSL, line: *const c_char) {
    let ex_data = match get_ex_data_from_ptr::<ExData>(ssl, *QUICHE_EX_DATA_INDEX)
    {
        Some(v) => v,

        None => return,
    };

    if let Some(keylog) = &mut ex_data.keylog {
        let data = unsafe { ffi::CStr::from_ptr(line).to_bytes() };

        let mut full_line = Vec::with_capacity(data.len() + 1);
        full_line.extend_from_slice(data);
        full_line.push(b'\n');

        keylog.write_all(&full_line[..]).ok();
        keylog.flush().ok();
    }
}

extern fn select_alpn(
    ssl: *mut SSL, out: *mut *const u8, out_len: *mut u8, inp: *mut u8,
    in_len: c_uint, _arg: *mut c_void,
) -> c_int {
    // SSL_TLSEXT_ERR_OK 0
    // SSL_TLSEXT_ERR_ALERT_WARNING 1
    // SSL_TLSEXT_ERR_ALERT_FATAL 2
    // SSL_TLSEXT_ERR_NOACK 3

    // Boringssl internally overwrite the return value from this callback, if the
    // returned value is SSL_TLSEXT_ERR_NOACK and is quic, then the value gets
    // overwritten to SSL_TLSEXT_ERR_ALERT_FATAL. In contrast openssl/quictls does
    // not do that, so we need to explicitly respond with
    // SSL_TLSEXT_ERR_ALERT_FATAL in case it is needed.
    // TLS_ERROR is redefined for each vendor.
    let ex_data = match get_ex_data_from_ptr::<ExData>(ssl, *QUICHE_EX_DATA_INDEX)
    {
        Some(v) => v,

        None => return TLS_ERROR,
    };

    if ex_data.application_protos.is_empty() {
        return TLS_ERROR;
    }

    let mut protos = octets::Octets::with_slice(unsafe {
        slice::from_raw_parts(inp, in_len as usize)
    });

    while let Ok(proto) = protos.get_bytes_with_u8_length() {
        let found = ex_data.application_protos.iter().any(|expected| {
            trace!(
                "checking peer ALPN {:?} against {:?}",
                std::str::from_utf8(proto.as_ref()),
                std::str::from_utf8(expected.as_slice())
            );

            if expected.len() == proto.len() &&
                expected.as_slice() == proto.as_ref()
            {
                unsafe {
                    *out = expected.as_slice().as_ptr();
                    *out_len = expected.len() as u8;
                }

                return true;
            }

            false
        });

        if found {
            return 0; // SSL_TLSEXT_ERR_OK
        }
    }

    TLS_ERROR
}

extern fn new_session(ssl: *mut SSL, session: *mut SSL_SESSION) -> c_int {
    let ex_data = match get_ex_data_from_ptr::<ExData>(ssl, *QUICHE_EX_DATA_INDEX)
    {
        Some(v) => v,

        None => return 0,
    };

    let handshake = Handshake::new(ssl);
    let peer_params = handshake.quic_transport_params();

    // Serialize session object into buffer.
    let session_bytes = match get_session_bytes(session) {
        Ok(v) => v,
        Err(_) => return 0,
    };

    let mut buffer =
        Vec::with_capacity(8 + peer_params.len() + 8 + session_bytes.len());

    let session_bytes_len = session_bytes.len() as u64;

    if buffer.write(&session_bytes_len.to_be_bytes()).is_err() {
        std::mem::forget(handshake);
        return 0;
    }

    if buffer.write(&session_bytes).is_err() {
        std::mem::forget(handshake);
        return 0;
    }

    let peer_params_len = peer_params.len() as u64;

    if buffer.write(&peer_params_len.to_be_bytes()).is_err() {
        std::mem::forget(handshake);
        return 0;
    }

    if buffer.write(peer_params).is_err() {
        std::mem::forget(handshake);
        return 0;
    }

    *ex_data.session = Some(buffer);

    // Prevent handshake from being freed, as we still need it.
    std::mem::forget(handshake);

    0
}

pub fn map_result(bssl_result: c_int) -> Result<()> {
    match bssl_result {
        1 => Ok(()),
        _ => Err(Error::TlsFail),
    }
}

pub fn map_result_zero_is_success(bssl_result: c_int) -> Result<()> {
    match bssl_result {
        0 => Ok(()),
        _ => Err(Error::TlsFail),
    }
}

pub fn map_result_ptr<'a, T>(bssl_result: *const T) -> Result<&'a T> {
    match unsafe { bssl_result.as_ref() } {
        Some(v) => Ok(v),
        None => Err(Error::TlsFail),
    }
}

fn log_ssl_error() {
    let mut err = [0u8; 1024];

    unsafe {
        let e = ERR_peek_error();
        ERR_error_string_n(e, err.as_mut_ptr() as *mut c_char, err.len());
    }

    trace!("{}", std::str::from_utf8(&err).unwrap());
}

extern {
    // Note: some vendor-specific methods are implemented by each vendor's
    // submodule (openssl-quictls / boringssl).

    // SSL_METHOD
    fn TLS_method() -> *const SSL_METHOD;

    // SSL_CTX
    fn SSL_CTX_new(method: *const SSL_METHOD) -> *mut SSL_CTX;
    fn SSL_CTX_free(ctx: *mut SSL_CTX);

    fn SSL_CTX_use_certificate_chain_file(
        ctx: *mut SSL_CTX, file: *const c_char,
    ) -> c_int;

    fn SSL_CTX_use_PrivateKey_file(
        ctx: *mut SSL_CTX, file: *const c_char, ty: c_int,
    ) -> c_int;

    fn SSL_CTX_load_verify_locations(
        ctx: *mut SSL_CTX, file: *const c_char, path: *const c_char,
    ) -> c_int;

    #[cfg(not(windows))]
    fn SSL_CTX_set_default_verify_paths(ctx: *mut SSL_CTX) -> c_int;

    #[cfg(windows)]
    fn SSL_CTX_get_cert_store(ctx: *mut SSL_CTX) -> *mut X509_STORE;

    fn SSL_CTX_set_verify(
        ctx: *mut SSL_CTX, mode: c_int,
        cb: Option<
            unsafe extern fn(ok: c_int, store_ctx: *mut X509_STORE_CTX) -> c_int,
        >,
    );

    fn SSL_CTX_set_keylog_callback(
        ctx: *mut SSL_CTX,
        cb: Option<unsafe extern fn(ssl: *const SSL, line: *const c_char)>,
    );

    fn SSL_CTX_set_alpn_protos(
        ctx: *mut SSL_CTX, protos: *const u8, protos_len: usize,
    ) -> c_int;

    fn SSL_CTX_set_alpn_select_cb(
        ctx: *mut SSL_CTX,
        cb: Option<
            unsafe extern fn(
                ssl: *mut SSL,
                out: *mut *const u8,
                out_len: *mut u8,
                inp: *mut u8,
                in_len: c_uint,
                arg: *mut c_void,
            ) -> c_int,
        >,
        arg: *mut c_void,
    );

    fn SSL_CTX_sess_set_new_cb(
        ctx: *mut SSL_CTX,
        cb: Option<
            unsafe extern fn(ssl: *mut SSL, session: *mut SSL_SESSION) -> c_int,
        >,
    );

    fn SSL_new(ctx: *mut SSL_CTX) -> *mut SSL;

    fn SSL_get_error(ssl: *const SSL, ret_code: c_int) -> c_int;

    fn SSL_set_accept_state(ssl: *mut SSL);
    fn SSL_set_connect_state(ssl: *mut SSL);

    fn SSL_get0_param(ssl: *mut SSL) -> *mut X509_VERIFY_PARAM;

    fn SSL_set_ex_data(ssl: *mut SSL, idx: c_int, ptr: *mut c_void) -> c_int;
    fn SSL_get_ex_data(ssl: *const SSL, idx: c_int) -> *mut c_void;

    fn SSL_get_current_cipher(ssl: *const SSL) -> *const SSL_CIPHER;

    fn SSL_set_session(ssl: *mut SSL, session: *mut SSL_SESSION) -> c_int;

    fn SSL_get_SSL_CTX(ssl: *const SSL) -> *mut SSL_CTX;

    fn SSL_set_quiet_shutdown(ssl: *mut SSL, mode: c_int);

    fn SSL_set_quic_transport_params(
        ssl: *mut SSL, params: *const u8, params_len: usize,
    ) -> c_int;

    fn SSL_set_quic_method(
        ssl: *mut SSL, quic_method: *const SSL_QUIC_METHOD,
    ) -> c_int;

    fn SSL_set_quic_use_legacy_codepoint(ssl: *mut SSL, use_legacy: c_int);

    #[cfg(test)]
    fn SSL_set_options(ssl: *mut SSL, opts: u32) -> u32;

    fn SSL_get_peer_quic_transport_params(
        ssl: *const SSL, out_params: *mut *const u8, out_params_len: *mut usize,
    );

    fn SSL_get0_alpn_selected(
        ssl: *const SSL, out: *mut *const u8, out_len: *mut u32,
    );

    fn SSL_get_servername(ssl: *const SSL, ty: c_int) -> *const c_char;

    fn SSL_provide_quic_data(
        ssl: *mut SSL, level: crypto::Level, data: *const u8, len: usize,
    ) -> c_int;

    fn SSL_process_quic_post_handshake(ssl: *mut SSL) -> c_int;

    fn SSL_do_handshake(ssl: *mut SSL) -> c_int;

    fn SSL_quic_write_level(ssl: *const SSL) -> crypto::Level;

    fn SSL_session_reused(ssl: *const SSL) -> c_int;

    fn SSL_in_init(ssl: *const SSL) -> c_int;

    fn SSL_clear(ssl: *mut SSL) -> c_int;

    fn SSL_free(ssl: *mut SSL);

    // SSL_CIPHER
    fn SSL_CIPHER_get_id(cipher: *const SSL_CIPHER) -> c_uint;

    // SSL_SESSION

    fn SSL_SESSION_free(session: *mut SSL_SESSION);

    // X509_VERIFY_PARAM
    fn X509_VERIFY_PARAM_set1_host(
        param: *mut X509_VERIFY_PARAM, name: *const c_char, namelen: usize,
    ) -> c_int;

    // X509_STORE
    #[cfg(windows)]
    fn X509_STORE_add_cert(ctx: *mut X509_STORE, x: *mut X509) -> c_int;

    // X509
    #[cfg(windows)]
    fn X509_free(x: *mut X509);
    #[cfg(windows)]
    fn d2i_X509(px: *mut X509, input: *const *const u8, len: c_int) -> *mut X509;

    // ERR
    fn ERR_peek_error() -> c_uint;

    fn ERR_error_string_n(err: c_uint, buf: *mut c_char, len: usize);

    // OPENSSL
    #[allow(dead_code)]
    fn OPENSSL_free(ptr: *mut c_void);

}

#[cfg(not(feature = "openssl"))]
mod boringssl;
#[cfg(not(feature = "openssl"))]
use boringssl::*;

#[cfg(feature = "openssl")]
mod openssl_quictls;
#[cfg(feature = "openssl")]
use openssl_quictls::*;
