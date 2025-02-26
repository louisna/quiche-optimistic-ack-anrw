//! This module attempts to simulate the start phase of the CUBIC congestion
//! control algorithm.

use jsonseq;
use std::error::Error;
use std::ops::Range;
use std::time::Duration;
use std::time::Instant;

use quiche::Connection;

pub struct OpportunistQlog {
    /// List of all optimist acknowledgments the client has to send.
    oack: Vec<(Duration, Vec<Range<u64>>)>,

    /// Time at which we start sending oack.
    start_time: Option<Instant>,

    /// Current index in the list.
    idx: usize,

    /// Last maximum packet that we acked.
    last_max_pn: u64,

    /// Whether the opportunist ack is currently active.
    active: bool,
}

impl OpportunistQlog {
    /// Creates a new instance.
    pub fn new(qlog_filename: &str) -> std::result::Result<Self, Box<dyn Error>> {
        Ok(Self {
            active: false,
            oack: Self::read_qlog(qlog_filename)?,
            start_time: None,
            idx: 0,
            last_max_pn: 0,
        })
    }

    /// Reads a QLOG file and generates the times at which we must send an ACK.
    /// This function is very ugly because I don't know how to properly read a
    /// JSONSeq file.
    fn read_qlog(
        qlog_filename: &str,
    ) -> std::result::Result<Vec<(Duration, Vec<Range<u64>>)>, Box<dyn Error>>
    {
        let file = std::fs::File::open(qlog_filename)?;
        let json_data = jsonseq::JsonSeqReader::new(file);
        println!("READ SQLOG");

        let mut ack_to_send = Vec::new();
        // We shift the packet numbers to 0 in case this trace was generated with
        // a server starting at a random packet number.
        let mut shift = None;

        for data in json_data {
            let d = data?;
            if d["name"].as_str() == Some("transport:packet_sent") {
                // Only acknowledgments for 'application' packet type;
                if d["data"]["header"]["packet_type"].as_str() != Some("1RTT") {
                    continue;
                }

                // Only get ACK frames.
                let frames_opt = d["data"]["frames"].as_array();

                if let Some(frames) = frames_opt {
                    for frame in frames.iter() {
                        if frame["frame_type"].as_str() == Some("ack") {
                            // Potentially send multiple ranges in the same
                            // packet.
                            let mut ranges = Vec::new();

                            for range in frame["acked_ranges"].as_array().unwrap()
                            {
                                let from = range[0].as_u64().unwrap();
                                if shift.is_none() {
                                    shift = Some(from);
                                    info!("This is the shift: {:?}", shift);
                                }
                                let to = range[1].as_u64().unwrap();
                                ranges.push(
                                    from - shift.unwrap()..
                                        to - shift.unwrap() + 1,
                                );
                            }

                            // Get the time at which we send.
                            let time_to_send = (d["time"].as_f64().unwrap() *
                                1_000_000.0)
                                as u64;
                            let time_to_send =
                                std::time::Duration::from_nanos(time_to_send);

                            ack_to_send.push((time_to_send, ranges));
                        }
                    }
                }
            }
        }

        // Start the acknowledgment procedure directly when the OACK is activated,
        // so we shift to 0 all values.
        let first_time = ack_to_send.first().unwrap().0;
        let ack_to_send = ack_to_send
            .drain(..)
            .map(|(t, a)| {
                (
                    t.saturating_sub(first_time)
                        .saturating_add(Duration::from_millis(10)),
                    a,
                )
            })
            .collect();

        Ok(ack_to_send)
    }

    pub fn timeout(&mut self) -> Option<Duration> {
        if !self.is_active() {
            return None;
        }

        let now = Instant::now();

        if self.start_time.is_none() {
            self.start_time = Some(now);
        }

        let start_time = self.start_time.unwrap();
        let next_timeout = self.oack.get(self.idx)?.0;

        Some(
            (start_time + next_timeout + Duration::from_micros(1))
                .duration_since(now),
        )
    }

    // We assume that each RTT, we double the amount of data that was received in
    // the previous RTT, i.e., that we still are in the slow start threshold.
    // Also update the RTT estimation.
    pub fn on_timeout(
        &mut self, conn: &mut Connection, stream_id: u64,
    ) -> Result<(), Box<dyn Error>> {
        if self.timeout() == Some(Duration::ZERO) {
            let now = Instant::now();
            if self.start_time.is_none() {
                self.start_time = Some(now);
            }

            let ranges =
                &self.oack.get(self.idx).ok_or("INDEX ERROR".to_string())?.1;

            // The app might start at a random packet number, like mvfst.
            let shift = conn.oack_get_first_pn();

            for range in ranges.iter() {
                let range_shift = 0 + shift..range.end + shift;
                info!("Insert acknowledgment for: {:?}", range_shift);
                conn.insert_oack_app(
                    range_shift,
                    now,
                    stream_id,
                    self.last_max_pn,
                )?;

                self.last_max_pn = range.end;
            }

            // Increase the index and set to false if we go out of bound.
            self.idx += 1;
            if self.idx >= self.oack.len() {
                self.set_active(false);
            }
        }

        Ok(())
    }

    pub fn set_active(&mut self, v: bool) {
        self.active = v;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

pub struct OpportunistMaxPn {
    /// The last maximum packet number we acknowledged.
    last_max_pn: u64,

    /// The new maximum packet number to acknowledge.
    next_max_pn: u64,

    /// Whether we are active.
    active: bool,
}

impl OpportunistMaxPn {
    /// Creates a new instance.
    pub fn new() -> Self {
        Self {
            last_max_pn: 0,
            next_max_pn: 0,
            active: false,
        }
    }

    pub fn timeout(&self) -> Option<Duration> {
        (self.last_max_pn != self.next_max_pn).then_some(Duration::ZERO)
    }

    pub fn on_timeout(
        &mut self, conn: &mut Connection, stream_id: u64,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(Duration::ZERO) = self.timeout() {
            let range = 0..self.next_max_pn;
            
            let now = Instant::now();
            conn.insert_oack_app(range, now, stream_id, self.last_max_pn)?;

            self.last_max_pn = self.next_max_pn;
        }

        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn set_active(&mut self, v: bool) {
        self.active = v;
    }

    pub fn on_new_max_recv_pn(&mut self, pn: u64) {
        self.next_max_pn = std::cmp::max(self.next_max_pn, pn);
    }
}

pub enum OpportunistAck {
    /// Using QLOG.
    QLOG(OpportunistQlog),

    /// Using the maximum received packet number.
    MaxPn(OpportunistMaxPn),
}

impl OpportunistAck {
    pub fn new(
        qlog_filename: Option<&str>,
    ) -> std::result::Result<Self, Box<dyn Error>> {
        if let Some(filename) = qlog_filename {
            Ok(Self::QLOG(OpportunistQlog::new(filename)?))
        } else {
            Ok(Self::MaxPn(OpportunistMaxPn::new()))
        }
    }

    pub fn timeout(&mut self) -> Option<Duration> {
        match self {
            Self::QLOG(q) => q.timeout(),
            Self::MaxPn(m) => m.timeout(),
        }
    }

    pub fn on_timeout(
        &mut self, conn: &mut Connection, stream_id: u64,
    ) -> Result<(), Box<dyn Error>> {
        match self {
            Self::QLOG(q) => q.on_timeout(conn, stream_id),
            Self::MaxPn(m) => m.on_timeout(conn, stream_id),
        }
    }

    pub fn is_active(&self) -> bool {
        match self {
            Self::QLOG(q) => q.is_active(),
            Self::MaxPn(m) => m.is_active(),
        }
    }

    pub fn set_active(&mut self, v: bool) {
        match self {
            Self::QLOG(q) => q.set_active(v),
            Self::MaxPn(m) => m.set_active(v),
        }
    }

    pub fn on_new_max_recv_pn(&mut self, pn: u64) {
        match self {
            Self::QLOG(_q) => (),
            Self::MaxPn(m) => m.on_new_max_recv_pn(pn),
        }
    }
}
