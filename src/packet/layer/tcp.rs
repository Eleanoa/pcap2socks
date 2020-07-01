use super::ipv4::Ipv4;
use super::{Layer, LayerType, LayerTypes};
use pnet::packet::tcp::{
    self, MutableTcpOptionPacket, MutableTcpPacket, TcpFlags, TcpOption, TcpOptionNumber,
    TcpOptionNumbers, TcpOptionPacket, TcpPacket,
};
use std::clone::Clone;
use std::cmp::min;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::net::Ipv4Addr;

/// Represents a TCP packet.
#[derive(Clone, Debug)]
pub struct Tcp {
    pub layer: tcp::Tcp,
    pub src: Ipv4Addr,
    pub dst: Ipv4Addr,
}

impl Tcp {
    /// Creates a `Tcp` represents a TCP ACK.
    pub fn new_ack(
        src: u16,
        dst: u16,
        sequence: u32,
        acknowledgement: u32,
        window: u16,
        ts: Option<(u32, u32)>,
        sacks: Option<&Vec<(u32, u32)>>,
    ) -> Tcp {
        let mut d_tcp = tcp::Tcp {
            source: src,
            destination: dst,
            sequence,
            acknowledgement,
            data_offset: 5,
            reserved: 0,
            flags: TcpFlags::ACK,
            window,
            checksum: 0,
            urgent_ptr: 0,
            options: vec![],
            payload: vec![],
        };
        // TCP options
        let is_ts = ts.is_some();
        let is_sacks = sacks.is_some() && sacks.unwrap().len() > 0;

        if is_ts && is_sacks {
            let ts = ts.unwrap();
            let sacks = sacks.unwrap();

            // Trim sacks
            let size = min(3, sacks.len());
            let mut vector = Vec::with_capacity(size * 2);
            for i in 0..size {
                vector.push(sacks[i].0);
                vector.push(sacks[i].1);
            }

            d_tcp.data_offset += 3 + vector.len() as u8;
            d_tcp.options.push(TcpOption::timestamp(ts.0, ts.1));
            d_tcp
                .options
                .push(TcpOption::selective_ack(vector.as_slice()));
        } else if is_ts {
            let ts = ts.unwrap();

            d_tcp.data_offset += 3;
            d_tcp.options.push(TcpOption::nop());
            d_tcp.options.push(TcpOption::nop());
            d_tcp.options.push(TcpOption::timestamp(ts.0, ts.1));
        } else if is_sacks {
            let sacks = sacks.unwrap();

            // Trim sacks
            let size = min(4, sacks.len());
            let mut vector = Vec::with_capacity(size * 2);
            for i in 0..size {
                vector.push(sacks[i].0);
                vector.push(sacks[i].1);
            }

            d_tcp.data_offset += 1 + vector.len() as u8;
            d_tcp.options.push(TcpOption::nop());
            d_tcp.options.push(TcpOption::nop());
            d_tcp
                .options
                .push(TcpOption::selective_ack(vector.as_slice()));
        }
        Tcp::from(d_tcp)
    }

    /// Creates a `Tcp` represents a TCP ACK/SYN.
    pub fn new_ack_syn(
        src: u16,
        dst: u16,
        sequence: u32,
        acknowledgement: u32,
        window: u16,
        mss: Option<u16>,
        wscale: Option<u8>,
        sack_perm: bool,
        ts: Option<(u32, u32)>,
    ) -> Tcp {
        let mut tcp = Tcp::new_ack(src, dst, sequence, acknowledgement, window, None, None);
        tcp.layer.flags |= TcpFlags::SYN;
        // TCP options
        if let Some(mss) = mss {
            tcp.layer.data_offset += 1;
            tcp.layer.options.push(TcpOption::mss(mss));
        }
        if let Some(wscale) = wscale {
            tcp.layer.data_offset += 1;
            tcp.layer.options.push(TcpOption::nop());
            tcp.layer.options.push(TcpOption::nop());
            tcp.layer.options.push(TcpOption::wscale(wscale));
        }
        let is_ts = ts.is_some();
        if sack_perm && is_ts {
            let ts = ts.unwrap();

            tcp.layer.data_offset += 3;
            tcp.layer.options.push(TcpOption::sack_perm());
            tcp.layer.options.push(TcpOption::timestamp(ts.0, ts.1));
        } else if sack_perm {
            tcp.layer.data_offset += 1;
            tcp.layer.options.push(TcpOption::nop());
            tcp.layer.options.push(TcpOption::nop());
            tcp.layer.options.push(TcpOption::sack_perm());
        } else if is_ts {
            let ts = ts.unwrap();

            tcp.layer.data_offset += 3;
            tcp.layer.options.push(TcpOption::nop());
            tcp.layer.options.push(TcpOption::nop());
            tcp.layer.options.push(TcpOption::timestamp(ts.0, ts.1));
        }

        tcp
    }

    /// Creates a `Tcp` represents a TCP ACK/RST.
    pub fn new_ack_rst(
        src: u16,
        dst: u16,
        sequence: u32,
        acknowledgement: u32,
        window: u16,
        ts: Option<(u32, u32)>,
    ) -> Tcp {
        let mut tcp = Tcp::new_rst(src, dst, sequence, acknowledgement, window, ts);
        tcp.layer.flags |= TcpFlags::ACK;
        tcp
    }

    /// Creates a `Tcp` represents a TCP ACK/FIN.
    pub fn new_ack_fin(
        src: u16,
        dst: u16,
        sequence: u32,
        acknowledgement: u32,
        window: u16,
        ts: Option<(u32, u32)>,
    ) -> Tcp {
        let mut tcp = Tcp::new_ack(src, dst, sequence, acknowledgement, window, ts, None);
        tcp.layer.flags |= TcpFlags::FIN;
        tcp
    }

    /// Creates a `Tcp` represents a TCP RST.
    pub fn new_rst(
        src: u16,
        dst: u16,
        sequence: u32,
        acknowledgement: u32,
        window: u16,
        ts: Option<(u32, u32)>,
    ) -> Tcp {
        let mut tcp = Tcp::new_ack(src, dst, sequence, acknowledgement, window, ts, None);
        tcp.layer.flags = TcpFlags::RST;
        tcp
    }

    /// Creates a `Tcp` according to the given `Tcp`.
    pub fn from(tcp: tcp::Tcp) -> Tcp {
        Tcp {
            layer: tcp,
            src: Ipv4Addr::UNSPECIFIED,
            dst: Ipv4Addr::UNSPECIFIED,
        }
    }

    /// Creates a `Tcp` according to the given TCP packet and the `Ipv4`.
    pub fn parse(packet: &TcpPacket, ipv4: &Ipv4) -> Tcp {
        let d_tcp = tcp::Tcp {
            source: packet.get_source(),
            destination: packet.get_destination(),
            sequence: packet.get_sequence(),
            acknowledgement: packet.get_acknowledgement(),
            data_offset: packet.get_data_offset(),
            reserved: packet.get_reserved(),
            flags: packet.get_flags(),
            window: packet.get_window(),
            checksum: packet.get_checksum(),
            urgent_ptr: packet.get_urgent_ptr(),
            options: packet.get_options(),
            payload: vec![],
        };
        let mut tcp = Tcp::from(d_tcp);
        tcp.set_ipv4_layer(ipv4);

        tcp
    }

    /// Sets the source and destination IP address for the layer with the given `Ipv4`.
    pub fn set_ipv4_layer(&mut self, ipv4: &Ipv4) {
        self.src = ipv4.get_src();
        self.dst = ipv4.get_dst();
    }

    /// Get the source IP address of the layer.
    pub fn get_src_ip_addr(&self) -> Ipv4Addr {
        self.src
    }

    /// Get the destination IP address of the layer.
    pub fn get_dst_ip_addr(&self) -> Ipv4Addr {
        self.dst
    }

    /// Get the source of the layer.
    pub fn get_src(&self) -> u16 {
        self.layer.source
    }

    /// Get the destination of the layer.
    pub fn get_dst(&self) -> u16 {
        self.layer.destination
    }

    /// Get the sequence of the layer.
    pub fn get_sequence(&self) -> u32 {
        self.layer.sequence
    }

    /// Get the acknowledgement of the layer.
    pub fn get_acknowledgement(&self) -> u32 {
        self.layer.acknowledgement
    }

    /// Get the string represents the flags of the layer.
    pub fn get_flag_string(&self) -> String {
        let mut flags = String::from("[");
        if self.is_syn() {
            flags = flags + "S";
        }
        if self.is_rst() {
            flags = flags + "R";
        }
        if self.is_fin() {
            flags = flags + "F";
        }
        if self.is_ack() {
            flags = flags + ".";
        }
        flags = flags + "]";

        flags
    }

    /// Get the window size of the layer.
    pub fn get_window(&self) -> u16 {
        self.layer.window
    }

    /// Get the selective acknowledgements of the layer. This function allocates space for serializing options.
    pub fn get_sack(&self) -> Option<Vec<(u32, u32)>> {
        let mut buffer = vec![0u8; 40];
        let mut packet = MutableTcpOptionPacket::new(buffer.as_mut_slice()).unwrap();
        for ref option in &self.layer.options {
            packet.populate(option);
            match packet.get_number() {
                TcpOptionNumbers::SACK => {
                    let mut vector = Vec::with_capacity(4);

                    let pair_length = (buffer[1] as usize - 2) / 8;
                    for i in 0..pair_length {
                        let left = bytes_to_u32(&buffer[2 + 8 * i..2 + 8 * i + 4]);
                        let right = bytes_to_u32(&buffer[2 + 8 * i + 4..2 + 8 * i + 8]);
                        vector.push((left, right));
                    }

                    return Some(vector);
                }
                _ => {}
            }
        }

        None
    }

    /// Get the timestamp of the layer. This function allocates space for serializing options.
    pub fn get_ts(&self) -> Option<u32> {
        let mut buffer = vec![0u8; 40];
        let mut packet = MutableTcpOptionPacket::new(buffer.as_mut_slice()).unwrap();
        for ref option in &self.layer.options {
            packet.populate(option);
            match packet.get_number() {
                TcpOptionNumbers::TIMESTAMPS => {
                    let ts = bytes_to_u32(&buffer[2..6]);

                    return Some(ts);
                }
                _ => {}
            }
        }

        None
    }

    /// Get the timestamp echo reply of the layer. This function allocates space for serializing options.
    pub fn get_ts_ecr(&self) -> Option<u32> {
        let mut buffer = vec![0u8; 40];
        let mut packet = MutableTcpOptionPacket::new(buffer.as_mut_slice()).unwrap();
        for ref option in &self.layer.options {
            packet.populate(option);
            match packet.get_number() {
                TcpOptionNumbers::TIMESTAMPS => {
                    let ts = bytes_to_u32(&buffer[6..10]);

                    return Some(ts);
                }
                _ => {}
            }
        }

        None
    }

    /// Returns if the `Tcp` is a TCP acknowledgement.
    pub fn is_ack(&self) -> bool {
        self.layer.flags & TcpFlags::ACK != 0
    }

    /// Returns if the `Tcp` is a TCP acknowledgement and finish.
    pub fn is_ack_fin(&self) -> bool {
        self.is_ack() && self.is_fin()
    }

    /// Returns if the `Tcp` is a TCP reset.
    pub fn is_rst(&self) -> bool {
        self.layer.flags & TcpFlags::RST != 0
    }

    /// Returns if the `Tcp` is a TCP synchronization.
    pub fn is_syn(&self) -> bool {
        self.layer.flags & TcpFlags::SYN != 0
    }

    /// Returns if the `Tcp` is a TCP finish.
    pub fn is_fin(&self) -> bool {
        self.layer.flags & TcpFlags::FIN != 0
    }

    /// Returns if the `Tcp` is a TCP reset or finish.
    pub fn is_rst_or_fin(&self) -> bool {
        self.is_rst() || self.is_fin()
    }

    /// Returns if the `Tcp` has zero window.
    pub fn is_zero_window(&self) -> bool {
        self.layer.window == 0
    }

    /// Returns if the `Tcp` indicates selective acknowledgements permitted. This function allocates space for serializing options.
    pub fn is_sack_perm(&self) -> bool {
        for ref option in &self.layer.options {
            match get_number_from_option(option) {
                TcpOptionNumbers::SACK_PERMITTED => return true,
                _ => {}
            }
        }

        false
    }
}

impl Display for Tcp {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "{}: {} -> {} {}",
            LayerTypes::Tcp,
            self.layer.source,
            self.layer.destination,
            self.get_flag_string()
        )
    }
}

impl Layer for Tcp {
    fn get_type(&self) -> LayerType {
        LayerTypes::Tcp
    }

    fn get_size(&self) -> usize {
        let mut tcp_size = TcpPacket::packet_size(&self.layer);
        let mut tcp_options_size = 0;
        for option in &self.layer.options {
            tcp_size -= 1;
            tcp_options_size += TcpOptionPacket::packet_size(option);
        }

        tcp_size + tcp_options_size
    }

    fn serialize(&self, buffer: &mut [u8], _: usize) -> io::Result<usize> {
        let mut packet = MutableTcpPacket::new(buffer)
            .ok_or(io::Error::new(io::ErrorKind::WriteZero, "buffer too small"))?;

        packet.populate(&self.layer);

        // Fix length
        let header_length = self.get_size();
        if header_length / 4 > u8::MAX as usize {
            return Err(io::Error::new(io::ErrorKind::Other, "TCP too big"));
        }
        packet.set_data_offset((header_length / 4) as u8);

        // Compute checksum
        let checksum = tcp::ipv4_checksum(
            &packet.to_immutable(),
            &self.get_src_ip_addr(),
            &self.get_dst_ip_addr(),
        );
        packet.set_checksum(checksum);

        Ok(header_length)
    }

    fn serialize_with_payload(
        &self,
        buffer: &mut [u8],
        payload: &[u8],
        n: usize,
    ) -> io::Result<usize> {
        let mut packet = MutableTcpPacket::new(buffer)
            .ok_or(io::Error::new(io::ErrorKind::WriteZero, "buffer too small"))?;

        packet.populate(&self.layer);

        // Copies payload
        packet.set_payload(payload);

        // Fix length
        let header_length = self.get_size();
        if header_length / 4 > u8::MAX as usize {
            return Err(io::Error::new(io::ErrorKind::Other, "TCP too big"));
        }
        packet.set_data_offset((header_length / 4) as u8);

        // Compute checksum
        let checksum = tcp::ipv4_checksum(
            &packet.to_immutable(),
            &self.get_src_ip_addr(),
            &self.get_dst_ip_addr(),
        );
        packet.set_checksum(checksum);

        Ok(header_length + n)
    }
}

fn bytes_to_u32(bytes: &[u8]) -> u32 {
    let mut result: u32 = 0;
    for i in 0..min(4, bytes.len()) {
        result = result * 256 + bytes[i] as u32;
    }
    result
}

fn get_number_from_option(option: &TcpOption) -> TcpOptionNumber {
    let buffer = vec![0u8; 40];
    let mut packet = MutableTcpOptionPacket::owned(buffer).unwrap();
    packet.populate(option);
    packet.get_number()

    /* An unsafe but faster solution (not completed)
    let r = option as *const TcpOption;
    unsafe {
        let tr: *const TcpOptionNumber = mem::transmute(r);

        return *tr;
    }
    */
}
