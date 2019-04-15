use log::{debug, error, info, warn};
use std::io::Result;
use std::net::*;
use trust_dns::op::{Header, MessageType, OpCode, ResponseCode};
use trust_dns::rr::{Name, RData, Record, RecordType};
use trust_dns_proto::rr::RrsetRecords;
use trust_dns_server::authority::authority::LookupRecords;
use trust_dns_server::authority::{AuthLookup, MessageRequest, MessageResponseBuilder};
use trust_dns_server::server::*;
use trust_dns_server::ServerFuture;

const LOCALHOST: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);

pub struct DNSServer {
  pub port: u16,
}

impl DNSServer {
  pub fn run(&self) {
    use futures::{future, Future};
    use tokio::runtime::current_thread::Runtime;
    use tokio_udp::*;

    let addr = SocketAddr::from((LOCALHOST, self.port));
    let handler = DNSHandler {};
    let server = ServerFuture::new(handler);
    let udp_socket = UdpSocket::bind(&addr).expect("error binding to UDP");
    let mut io_loop = Runtime::new().expect("error creating tokio runtime");

    let server_future: Box<Future<Item = (), Error = ()> + Send> =
      Box::new(future::lazy(move || {
        info!("binding UDP socket");
        server.register_socket(udp_socket);
        future::empty()
      }));
    io_loop.block_on(server_future).unwrap()
  }
}

pub struct DNSHandler;

impl DNSHandler {
  fn resolve<'q, R: ResponseHandler + 'static>(
    &self,
    request: &'q MessageRequest,
    response_handle: R,
  ) -> Result<()> {
    // for now we only care about first request
    // TODO better handle no queries
    let query = request
      .queries()
      .get(0)
      .expect("failed to get first query from dns request");
    let mut response = MessageResponseBuilder::new(Some(request.raw_queries()));
    let name = query.name().to_string();
    if !name.ends_with("test.") {
      warn!("we don't handle this domain");
      return response_handle.send_response(response.error_msg(
        request.id(),
        request.op_code(),
        ResponseCode::NXDomain,
      ));
    }

    debug!("first query is {}", name);
    // TODO: TSIG?
    let record = match query.query_type() {
      RecordType::A => Record::from_rdata(
        Name::from(query.name().clone()),
        0,
        RecordType::A,
        RData::A(Ipv4Addr::LOCALHOST),
      ),
      RecordType::AAAA => Record::from_rdata(
        Name::from(query.name().clone()),
        0,
        RecordType::AAAA,
        RData::AAAA(Ipv6Addr::LOCALHOST),
      ),
      query_type => {
        warn!(
          "received unsupported query type ({:?}) for {}",
          query_type, name
        );
        return response_handle.send_response(response.error_msg(
          request.id(),
          request.op_code(),
          ResponseCode::NXRRSet,
        ));
      }
    };
    let record_vec = vec![record];
    let ro = RrsetRecords::RecordsOnly(record_vec.iter());
    let records = LookupRecords::RecordsIter(ro);
    let answer = AuthLookup::Records(records);
    let mut response_header = Header::new();
    response_header.set_id(request.id());
    response_header.set_op_code(OpCode::Query);
    response_header.set_message_type(MessageType::Response);
    response_header.set_response_code(ResponseCode::NoError);
    response_header.set_authoritative(true);
    response_header.set_recursion_available(false);
    response.answers(answer);
    return response_handle.send_response(response.build(response_header));
  }
}

impl RequestHandler for DNSHandler {
  fn handle_request<'q, 'a, R: ResponseHandler + 'static>(
    &'a self,
    request: &'q Request,
    response_handle: R,
  ) -> Result<()> {
    let message = &request.message;
    debug!("req: {:?}", &message);

    // TODO: should we check for EDNS?

    let response = MessageResponseBuilder::new(Some(message.raw_queries()));
    match message.message_type() {
      MessageType::Query => match message.op_code() {
        OpCode::Query => return self.resolve(message, response_handle),
        code => {
          error!("unimplemented opcode: {:?}", code);
          return response_handle.send_response(response.error_msg(
            message.id(),
            message.op_code(),
            ResponseCode::NotImp,
          ));
        }
      },
      MessageType::Response => {
        warn!("got a response as request from id: {}", message.id());
        return response_handle.send_response(response.error_msg(
          message.id(),
          message.op_code(),
          ResponseCode::FormErr,
        ));
      }
    }
  }
}
