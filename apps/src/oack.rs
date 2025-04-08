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

    /// Potential shift with respect to the maximum received packet number.
    shift_pn: Option<u64>,

    /// Potential lag before adding shifting to the maximum received packet
    /// number.
    lag_shift: Option<u64>,

    /// The true last maximum packet number we received.
    true_last_recv_pn: u64,

    /// The true last maximum packet number at which we sent an OACK.
    true_last_pn_oack: u64,

    /// First application packet number.
    first_app_pn: Option<u64>,

    /// Last increase of the `shift_pn`.
    last_increase: u64,
}

impl OpportunistMaxPn {
    /// Creates a new instance.
    pub fn new(shift_pn: Option<u64>, lag_shift: Option<u64>) -> Self {
        Self {
            last_max_pn: 0,
            next_max_pn: 0,
            active: false,
            shift_pn,
            lag_shift: shift_pn.map(|_| lag_shift).flatten(),
            true_last_recv_pn: 0,
            true_last_pn_oack: 0,
            first_app_pn: None,
            last_increase: 0,
        }
    }

    pub fn timeout(&self) -> Option<Duration> {
        (self.true_last_recv_pn != self.true_last_pn_oack)
            .then_some(Duration::ZERO)
    }

    pub fn on_timeout(
        &mut self, conn: &mut Connection, stream_id: u64,
    ) -> Result<(), Box<dyn Error>> {
        if self.first_app_pn.is_none() {
            return Ok(());
        }

        // Avoid injecting acknowledgment if we do not have a first non-zero
        // value.
        if Some(self.next_max_pn) < self.first_app_pn {
            return Ok(());
        }

        if let Some(Duration::ZERO) = self.timeout() {
            let range = self.first_app_pn.unwrap()..self.next_max_pn;
            println!(
                "Send OACK ranges: {:?} and max received is {} so the diff is {}. Shift={:?}",
                range,
                self.true_last_recv_pn,
                self.next_max_pn.saturating_sub(self.true_last_pn_oack),
                self.shift_pn
            );

            let now = Instant::now();
            let _ = conn.insert_oack_app(range, now, stream_id, self.last_max_pn);

            self.last_max_pn = self.next_max_pn;
            self.true_last_pn_oack = self.true_last_recv_pn;
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
        // Avoid computing twice the same.
        if pn == self.true_last_recv_pn {
            return;
        }

        self.true_last_recv_pn = pn;
        if self.first_app_pn.is_none() && pn > 0 {
            self.first_app_pn = Some(pn);
        }
        if self.first_app_pn.is_none() {
            return;
        }

        let shift_pn = self
            .lag_shift
            .map(|lag| lag + self.first_app_pn.unwrap() < pn)
            .unwrap_or(true)
            .then_some(self.shift_pn)
            .flatten();
        // println!("Recv packet: {pn} so will we shift: {:?} + {:?} < {:?} =
        // {:?}", self.lag_shift, self.first_app_pn, pn, shift_pn.is_some());
        if let Some(n) = shift_pn {
            let how_much = if n < 100 {
                n + 2
            } else if n < 4_000 {
                // (n as f64 * 1.03) as u64
                n + 2
            } else {
                n + self.last_increase
            };

            // Use this with neqo.
            // let how_much = if pn % 2 == 0 { n + 2 } else { n };

            let new_shift = Some(how_much);
            self.last_increase =
                new_shift.unwrap().saturating_sub(self.shift_pn.unwrap());
            self.shift_pn = new_shift;

            // self.shift_pn = self.shift_pn.map(|n| std::cmp::min(n + 5,
            // 100_000_000));
            self.next_max_pn =
                std::cmp::max(self.next_max_pn, pn + self.shift_pn.unwrap());
            // println!("Set next_max_pn={} because pn={} and shift={}",
            // self.next_max_pn, pn, shift);
        }
    }
}

/// Optimistic acknowledgment with a constant range.
/// Used to test if implementations do verify the correctness of the received
/// ACK frames.
pub struct OpportunistConstant {
    /// Range to advertise.
    range: Range<u64>,

    /// Whether a new packet was received since the last acknowledgment.
    new_recv: bool,

    /// First application packet number.
    first_app_pn: Option<u64>,

    /// Lag before sending the OACK.
    lag: Option<u64>,
}

impl OpportunistConstant {
    pub fn new(range: Range<u64>, lag: Option<u64>) -> Self {
        Self {
            range,
            new_recv: false,
            first_app_pn: None,
            lag,
        }
    }

    fn timeout(&self) -> Option<Duration> {
        self.new_recv.then_some(Duration::ZERO)
    }

    fn on_timeout(
        &mut self, conn: &mut Connection, stream_id: u64,
    ) -> Result<(), Box<dyn Error>> {
        if self.timeout() == Some(Duration::ZERO) {
            let now = Instant::now();
            println!("Insert range: {:?}", self.range);
            self.new_recv = false;
            conn.insert_oack_app(
                self.range.clone(),
                now,
                stream_id,
                self.range.end,
            )?;
        }

        Ok(())
    }

    fn is_active(&self) -> bool {
        true
    }

    fn set_active(&mut self) {
        ()
    }

    fn on_new_max_recv_pn(&mut self, pn: u64) {
        if self.first_app_pn.is_none() {
            self.first_app_pn = Some(pn);
            self.range = self.range.start.saturating_add(pn)..
                self.range.end.saturating_add(pn);
        }

        if Some(pn.saturating_sub(self.first_app_pn.unwrap())) > self.lag {
            self.new_recv = true;
        }
    }
}

pub enum OpportunistAck {
    /// Using QLOG.
    QLOG(OpportunistQlog),

    /// Using the maximum received packet number.
    /// Potentially shift the maximum received packet number with som lag.
    MaxPn(OpportunistMaxPn),

    /// Constant range.
    Constant(OpportunistConstant),
}

impl OpportunistAck {
    pub fn new(
        qlog_filename: Option<&str>, shift_pn: Option<u64>,
        lag_shift: Option<u64>,
    ) -> std::result::Result<Self, Box<dyn Error>> {
        if let Some(filename) = qlog_filename {
            Ok(Self::QLOG(OpportunistQlog::new(filename)?))
        } else {
            Ok(Self::MaxPn(OpportunistMaxPn::new(shift_pn, lag_shift)))
        }
    }

    pub fn timeout(&mut self) -> Option<Duration> {
        match self {
            Self::QLOG(q) => q.timeout(),
            Self::MaxPn(m) => m.timeout(),
            Self::Constant(c) => c.timeout(),
        }
    }

    pub fn on_timeout(
        &mut self, conn: &mut Connection, stream_id: u64,
    ) -> Result<(), Box<dyn Error>> {
        match self {
            Self::QLOG(q) => q.on_timeout(conn, stream_id),
            Self::MaxPn(m) => m.on_timeout(conn, stream_id),
            Self::Constant(c) => c.on_timeout(conn, stream_id),
        }
    }

    pub fn is_active(&self) -> bool {
        match self {
            Self::QLOG(q) => q.is_active(),
            Self::MaxPn(m) => m.is_active(),
            Self::Constant(c) => c.is_active(),
        }
    }

    pub fn set_active(&mut self, v: bool) {
        match self {
            Self::QLOG(q) => q.set_active(v),
            Self::MaxPn(m) => m.set_active(v),
            Self::Constant(c) => c.set_active(),
        }
    }

    pub fn on_new_max_recv_pn(&mut self, pn: u64) {
        match self {
            Self::QLOG(_q) => (),
            Self::MaxPn(m) => m.on_new_max_recv_pn(pn),
            Self::Constant(c) => c.on_new_max_recv_pn(pn),
        }
    }
}
