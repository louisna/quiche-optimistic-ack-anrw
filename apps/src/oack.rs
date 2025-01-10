//! This module attempts to simulate the start phase of the CUBIC congestion
//! control algorithm.

use std::error::Error;
use std::time::Duration;
use std::time::Instant;

use quiche::Connection;

const INITIAL_CONGESTION_WINDOW_PACKETS: u64 = 10;

pub struct OpportunistAck {
    /// Whether the opportunist ack is currently active.
    active: bool,

    /// Target bitrate.
    target_bitrate: f64,

    /// Smoothed RTT. Used for timeout.
    smoothed_rtt: Option<Duration>,

    /// Last timeout.
    last_timeout: Option<Instant>,

    /// Number of packets acknowledged in the last RTT flight.
    nb_pkt_oack: u64,

    /// Last (highest) acknowledged packet number.
    highest_oack_pn: u64,
}

impl OpportunistAck {
    pub fn new(target_bitrate: f64, nb_bytes: u64) -> Self {
        Self {
            active: false,
            target_bitrate,
            nb_bytes,
            smoothed_rtt: None,
            last_timeout: None,
            nb_pkt_oack: INITIAL_CONGESTION_WINDOW_PACKETS,
            highest_oack_pn: 0,
        }
    }

    pub fn set_active(&mut self, v: bool, conn: &mut Connection) {
        self.active = v;
        if v {
            println!("SET THE SMOOTHED RTT TO: {:?}", conn.oack_get_rtt());
            self.smoothed_rtt = conn.oack_get_rtt();
            self.last_timeout = Some(Instant::now());
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn timeout(&self) -> Option<Duration> {
        if !self.active || self.smoothed_rtt.is_none() {
            return None;
        }

        if self.last_timeout.is_none() {
            return Some(Duration::ZERO);
        }

        let now = Instant::now();

        let timeout = Some(
            (self.last_timeout.unwrap() + self.smoothed_rtt.unwrap()).duration_since(now)
        );

        info!("Timeout for oack is {:?}", timeout);

        return timeout;
    }

    // We assume that each RTT, we double the amount of data that was received in
    // the previous RTT, i.e., that we still are in the slow start threshold.
    // Also update the RTT estimation.
    pub fn on_timeout(
        &mut self, conn: &mut Connection, stream_id: u64,
    ) -> Result<(), Box<dyn Error>> {
        if self.timeout() == Some(Duration::ZERO) {
            let now = Instant::now();
    
            // Update last timeout.
            self.last_timeout = Some(Instant::now());

            let new_highest = self.highest_oack_pn + self.nb_pkt_oack;

            // Only double until we reach the target bitrate.
            // The bitrate is just an estimation.
            let bitrate_estimation = (1350 * 8 * self.nb_pkt_oack * 1000) as f64 / self.smoothed_rtt.unwrap().as_millis() as f64;
            info!("Bitrate estimation: {:?} vs target_bitrate: {:?} because pkt oack={:?}", bitrate_estimation, self.target_bitrate, self.nb_pkt_oack);
            if bitrate_estimation < self.target_bitrate {
                self.nb_pkt_oack = (self.nb_pkt_oack as f64 * 1.1) as u64;
            }

            info!("Insert 0..{new_highest}");
            conn.insert_oack_app(0..new_highest, now, stream_id, self.highest_oack_pn)?;
            self.highest_oack_pn = new_highest;

            // // Set the opportunist ack to false if we acked the whole data already.
            // if new_highest * 1350 >= self.nb_bytes {
            //     println!("SET MY OACK TO FALSE");
            //     self.active = false;
            // }
        }

        Ok(())
    }
}
