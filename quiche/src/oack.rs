//! This module adds control to do opportunist acknowedgments.
use std::cmp;
use std::ops::Range;
use std::time;
use std::time::Duration;

use crate::packet::Epoch;
use crate::ranges::RangeSet;
use crate::Connection;
use crate::Error;

impl Connection {
    /// Returns the packet numbers that were received since the last call.
    /// This method works as a `take()`, removing state from the callee.
    pub(crate) fn oack_take_new_pn(&mut self, epoch: Epoch) -> Option<RangeSet> {
        let was_some = self.pkt_num_spaces[epoch].oack_new_recv_pn.is_some();
        let res = self.pkt_num_spaces[epoch].oack_new_recv_pn.take();
        if was_some {
            self.pkt_num_spaces[epoch].oack_new_recv_pn =
                Some(RangeSet::default());
        }

        res
    }

    /// Returns the packet numbers that were received since the last call.
    /// This method works as a `take()`, removing state from the callee.
    pub fn oack_take_new_pn_app(&mut self) -> Option<RangeSet> {
        self.oack_take_new_pn(Epoch::Application)
    }

    /// Inserts a new range of packet numbers in the list of received pn.
    pub(crate) fn oack_insert_range_pn(
        &mut self, range: Range<u64>, epoch: Epoch,
    ) {
        if let Some(oack_pn) =
            self.pkt_num_spaces[epoch].oack_new_recv_pn.as_mut()
        {
            oack_pn.insert(range);
        }
    }

    /// Enables the opportunist acknowledgment extension by logging received
    /// packet numbers.
    pub fn enable_oack(&mut self, factor: u64) {
        for epoch in [Epoch::Handshake, Epoch::Application] {
            self.pkt_num_spaces[epoch].oack_new_recv_pn =
                Some(RangeSet::default());
        }
        self.oack_scaling_factor = Some(factor);
    }

    /// Whether opportunist acknowledgments is enabled.
    pub fn is_oack_enabled(&self) -> bool {
        self.pkt_num_spaces[Epoch::Application]
            .oack_new_recv_pn
            .is_some()
    }

    /// Insert new opportunist acknowledgments in the connection in the
    /// application [`Epoch`].
    ///
    /// This function basically does the same as the end of
    /// [`Connection::recv_single`] regarding the reception of packets.
    pub fn insert_oack_app(
        &mut self, range: Range<u64>, now: time::Instant, stream_id: u64,
        last_max_pn: u64,
    ) -> crate::Result<()> {
        let epoch = Epoch::Application;
        let recv_pid = 0;
        let payload_size = 1200;

        if !self.handshake_completed {
            println!("Trying to send opportunist acks but not handshake done.");
            return Ok(());
        }

        for pn in range {
            // We only record the time of arrival of the largest packet number
            // that still needs to be acked, to be used for ACK delay calculation.
            if self.pkt_num_spaces[epoch].recv_pkt_need_ack.last() < Some(pn) {
                self.pkt_num_spaces[epoch].largest_rx_pkt_time = now;
            }

            self.pkt_num_spaces[epoch].recv_pkt_num.insert(pn);

            self.pkt_num_spaces[epoch].recv_pkt_need_ack.push_item(pn);

            self.pkt_num_spaces[epoch].ack_elicited = true;

            self.pkt_num_spaces[epoch].largest_rx_pkt_num =
                cmp::max(self.pkt_num_spaces[epoch].largest_rx_pkt_num, pn);

            // Update send capacity.
            self.update_tx_cap();
            
            if pn > last_max_pn {
                self.recv_count += 1;
                self.paths.get_mut(recv_pid)?.recv_count += 1;
    
                self.recv_bytes += payload_size as u64;
                self.paths.get_mut(recv_pid)?.recv_bytes += payload_size as u64;
    
                self.ack_eliciting_sent = false;
                let stream = match self.get_or_create_stream(stream_id, false)
                {     Ok(v) => v,

                    Err(crate::Error::Done) => {println!("Error done for stream?"); return Ok(())},

                    Err(e) => { trace!("Impossible to get or create the stream {stream_id} because {:?}", e); return Err(e)},
                };

                let was_draining = stream.recv.is_draining();
                let max_off_delta = payload_size;
                let max_rx_data_left = self.max_rx_data() - self.rx_data;
                // println!("Max rx data left={max_rx_data_left} because {} - {} and max_off_delta={}", self.max_rx_data(), self.rx_data, max_off_delta);
                if max_off_delta > max_rx_data_left {
                    // println!("Error flow control");
                    return Err(Error::FlowControl);
                }
                self.rx_data += payload_size;
                // println!("Update rx_data to {}", self.rx_data);

                if was_draining {
                    // When a stream is in draining state it will not queue
                    // incoming data for the application to read, so consider
                    // the received data as consumed, which might trigger a flow
                    // control update.
                    self.flow_control.add_consumed(payload_size);

                    if self.should_update_max_data() {
                        self.almost_full = true;
                    }
                }

                // We must also update the flow control.
                // Overall flow control.
                self.flow_control.add_consumed(payload_size);
                if self.should_update_max_data() {
                    self.almost_full = true;
                }

                // Stream flow control.
                // Update the stream is almost full.
                let stream = self.get_or_create_stream(stream_id, false).unwrap();
                stream.recv.oack_add_consumed(payload_size);

                if stream.recv.almost_full() {
                    self.streams.insert_almost_full(stream_id);
                }
            }
        }

        Ok(())
    }

    /// Returns the (smoothed) RTT of the first path of the connection.
    pub fn oack_get_rtt(&self) -> Option<Duration> {
        let path = self.paths.get(0).ok()?;
        Some(path.recovery.rtt())
    }

    /// Schedule the sending of new MAX_DATA and MAX_STREAM_DATA frames.
    pub fn oack_new_max_data(&mut self, stream_id: u64) {
        if self.is_oack_enabled() {
            self.almost_full = true;
            self.streams.insert_almost_full(stream_id);
        }
    }

    #[inline]
    /// Return the first packet number received on the Application epoch.
    pub fn oack_get_first_pn(&self) -> u64 {
        self.oack_first_app_pn.unwrap_or(0)
    }

    #[inline]
    /// Returns the maximum packet number received on the first path on the Application epoch.
    pub fn oack_get_max_pn(&self) -> u64 {
        self.pkt_num_spaces[Epoch::Application].largest_rx_pkt_num
    }
}
