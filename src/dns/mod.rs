mod protocol;

use protocol::*;

use std::io::{self, Result};
use std::net::{Ipv4Addr, SocketAddr};

use futures::future::Future;
use futures::try_ready;
use log::{debug, trace, warn};
use tokio::net::UdpSocket;
use tokio::prelude::*;

pub struct DNSServer {
    socket: UdpSocket,
}

impl DNSServer {
    pub fn new(port: u16) -> Result<DNSServer> {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let socket = UdpSocket::bind(&addr)?;
        Ok(DNSServer { socket })
    }
}

impl Future for DNSServer {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        loop {
            let mut req_buffer = BytePacketBuffer::new();
            let to_send = Some(try_ready!(self.socket.poll_recv_from(&mut req_buffer.buf)));
            let request = DnsPacket::from_buffer(&mut req_buffer)?;
            debug!("received request {:#?}", &request.questions);
            let mut response = lookup(&request)?;
            if let Some((size, peer)) = to_send {
                let mut res_buffer = BytePacketBuffer::new();
                response.write(&mut res_buffer)?;
                let len = res_buffer.pos();
                let data = res_buffer.get_range(0, len)?;
                let amt = try_ready!(self.socket.poll_send_to(data, &peer));
                debug!("Sent {}/{} response bytes to {}", amt, size, peer);
            }
        }
    }
}

fn lookup(request: &DnsPacket) -> Result<DnsPacket> {
    let id = &request.header.id;
    trace!("received query (id: {}): {:?}", &id, &request);
    let mut response = DnsPacket::new();
    response.header.response = true;
    response.header.id = *id;
    response.header.recursion_desired = request.header.recursion_desired;

    if request.questions.is_empty() {
        response.header.rescode = ResultCode::NOTIMP;
        return Ok(response);
    }

    let query = &request.questions[0];
    response.questions.push(query.clone());

    if request.header.response {
        warn!("received response as question (id: {})", &id);
        response.header.rescode = ResultCode::NOTIMP;
        return Ok(response);
    }

    if request.header.opcode != 0 {
        warn!("received non-zero opcode (id: {})", &id);
        response.header.rescode = ResultCode::NOTIMP;
        return Ok(response);
    }

    if !query.name.ends_with(".test") {
        warn!("unsupported domain (id: {}): {}", &id, &query.name);
        response.header.rescode = ResultCode::SERVFAIL;
        return Ok(response);
    }

    match &query.qtype {
        QueryType::A => {
            let record = DnsRecord::A {
                addr: Ipv4Addr::LOCALHOST,
                domain: query.name.to_string(),
                ttl: 0,
            };
            response.answers.push(record);
        }
        QueryType::AAAA | QueryType::CNAME | QueryType::MX | QueryType::NS | QueryType::SOA => {
            debug!("received request for undefined query type: {:?}", &query);
            response.header.rescode = ResultCode::NOERROR;
        }
        QueryType::UNKNOWN(x) => {
            warn!("received query of unsupported type ({}): {:?}", x, &query);
            response.header.rescode = ResultCode::SERVFAIL;
        }
    }
    debug!("response is: {:#?}", &response);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::lookup;
    use super::protocol::*;
    use std::net::Ipv4Addr;

    macro_rules! lookup_tests {
        ($name:ident, $query_packet:expr, $response_code:expr, $extra_tests:expr) => {
            #[test]
            fn $name() {
                let response = lookup($query_packet).unwrap();
                // a few common tests
                assert_eq!($query_packet.header.id, response.header.id);
                assert_eq!(response.header.rescode, $response_code);
                // provided test function
                $extra_tests(&response);
            }
        };
    }

    lookup_tests! {
      normal_dns_request,
      {
        let mut packet = packet_with_question("hello.test".to_string(), QueryType::A);
        packet.header.recursion_desired = true;
        &packet.clone()
      },
      ResultCode::NOERROR,
      |response: &DnsPacket| {
        assert_eq!(response.header.recursion_desired, true);
        assert_eq!(
          response.questions[0].name, "hello.test",
          "response question's name doesn't match original name"
        );
        assert_eq!(
          response.answers[0],
          DnsRecord::A {
            domain: "hello.test".to_string(),
            addr: Ipv4Addr::LOCALHOST,
            ttl: 0
          }
        );
      }
    }

    lookup_tests! {
      subdomain_a_requests_are_supported,
      &packet_with_question("sub.domain.test".to_string(), QueryType::A),
      ResultCode::NOERROR,
      |response: &DnsPacket| {
        assert_eq!(
          response.answers[0],
          DnsRecord::A {
            domain: "sub.domain.test".to_string(),
            addr: Ipv4Addr::LOCALHOST,
            ttl: 0
          }
        );
      }
    }

    lookup_tests! {
      soa_requests_return_no_error_and_zero_answers,
      &packet_with_question("test.test".to_string(), QueryType::SOA),
      ResultCode::NOERROR,
      |response: &DnsPacket| {
        assert_eq!(response.answers.len(), 0);
      }
    }

    lookup_tests! {
      ns_requests_return_no_error_and_zero_answers,
      &packet_with_question("test.test".to_string(), QueryType::NS),
      ResultCode::NOERROR,
      |response: &DnsPacket| {
        assert_eq!(response.answers.len(), 0);
      }
    }

    lookup_tests! {
      packets_with_no_queries_are_not_implemented,
      {
        let mut packet = DnsPacket::new();
        packet.header.id = 1234;
        &packet.clone()
      },
      ResultCode::NOTIMP,
      |_| {}
    }

    lookup_tests! {
      response_packets_are_not_supported,
      {
        let mut packet = packet_with_question("test.test".to_string(), QueryType::A);
        packet.header.response = true;
        &packet.clone()
      },
      ResultCode::NOTIMP,
      |_| {}
    }

    lookup_tests! {
      non_zero_opcode_are_not_supported,
      {
        let mut packet = packet_with_question("test.test".to_string(), QueryType::A);
        packet.header.opcode = 1;
        &packet.clone()
      },
      ResultCode::NOTIMP,
      |_| {}
    }

    lookup_tests! {
      does_not_accept_wrong_domain,
      &packet_with_question("example.com".to_string(), QueryType::A),
      ResultCode::SERVFAIL,
        |response: &DnsPacket| {
          assert_eq!(response.answers.len(), 0);
        }
    }

    fn packet_with_question(name: String, query_type: QueryType) -> DnsPacket {
        let mut packet = DnsPacket::new();
        packet.header.id = 10;
        packet.questions.push(DnsQuestion::new(name, query_type));
        packet.clone()
    }
}
