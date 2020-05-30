//! SCTP packets are encapsulated into a lower-layer protocol (LLP) for transmission.  Using UDP as
//! an LLP is convenient for testing interoperability with libusrsctp, although for WebRTC data
//! channels the lower layer protocol will be DTLS.

use bytes::{BufMut, Bytes, BytesMut};
use futures::{Async, AsyncSink, Poll, Sink, StartSend, Stream};
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::UdpSocket;

use super::Packet;

pub struct LowerLayerPacket {
    pub buffer: Bytes,
    pub address: SocketAddr,
    // TODO: LLP-specific parameters, e.g. UDP encaps destination port?
    // (Instead of SocketAddr containing the port, which shouldn't be a concern at this layer.)
}

/// Render an SCTP packet into a lower layer packet.
pub fn packet_to_lower_layer(packet: &Packet) -> LowerLayerPacket {
    let destination = packet.llp_address;
    let rendered = packet.sctp_packet.write().unwrap();
    LowerLayerPacket {
        buffer: Bytes::from(rendered),
        address: destination,
    }
}

pub trait LowerLayerProtocol: Stream + Sink + Send {
    fn address(&self) -> SocketAddr;
}
pub type LowerLayer = dyn LowerLayerProtocol<
    Item = LowerLayerPacket,
    Error = io::Error,
    SinkItem = LowerLayerPacket,
    SinkError = io::Error,
>;

pub struct UdpLowerLayer {
    socket: UdpSocket,
    address: SocketAddr,
}

impl UdpLowerLayer {
    // The IANA-assigned UDP port number for encapsulating SCTP, as defined in RFC 6951.
    pub const SCTP_UDP_TUNNELING_PORT: u16 = 9899;
    // For testing purposes, we use this as the destination UDP port for outgoing connections.
    // TODO: Get rid of this..
    pub const SCTP_UDP_TUNNELING_PORT_OUTGOING: u16 = 9900;

    pub fn new() -> UdpLowerLayer {
        let localhost = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let address = SocketAddr::new(localhost, Self::SCTP_UDP_TUNNELING_PORT);
        // Open a UDP socket in non-blocking mode bound to IPv4 localhost port 9899.
        let socket = UdpSocket::bind(&address).unwrap();
        UdpLowerLayer {
            socket: socket,
            address: address,
        }
    }
}

impl LowerLayerProtocol for UdpLowerLayer {
    fn address(&self) -> SocketAddr {
        self.address
    }
}

impl Stream for UdpLowerLayer {
    type Item = LowerLayerPacket;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<LowerLayerPacket>, io::Error> {
        // TODO: Don't hard-code 1500; at least use a constant until we implement proper Path MTU
        // discovery.
        // TODO: Why are we allocating 1500 bytes on the stack?  There are so many ways to avoid
        // this... For example, use a buffer pool to recycle buffers, std::mem::replace() a Vec
        // member as needed, etc.  A LowerLayer is only created once for the lifetime of the stack,
        // so we shouldn't be afraid of some heap allocations here.
        let mut buffer: [u8; 1500] = [0; 1500];
        match self.socket.poll_recv_from(&mut buffer) {
            Ok(Async::Ready((nbytes, address))) => {
                let mut bytes = BytesMut::with_capacity(nbytes);
                bytes.put_slice(&buffer[..nbytes]);
                Ok(Async::Ready(Some(LowerLayerPacket {
                    buffer: bytes.freeze(),
                    address: address,
                })))
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(Async::NotReady),
            Err(e) => Err(e),
        }
    }
}

impl Sink for UdpLowerLayer {
    type SinkItem = LowerLayerPacket;
    type SinkError = io::Error;

    fn start_send(
        &mut self,
        packet: LowerLayerPacket,
    ) -> StartSend<Self::SinkItem, Self::SinkError> {
        match self.socket.poll_send_to(&packet.buffer, &packet.address) {
            Ok(Async::Ready(_)) => Ok(AsyncSink::Ready),
            Ok(Async::NotReady) => Ok(AsyncSink::NotReady(packet)),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(AsyncSink::NotReady(packet)),
            Err(e) => Err(e),
        }
    }
    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(Async::Ready(()))
    }
}
