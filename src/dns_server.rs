use std::net::*;
use std::str::FromStr;
use trust_dns_server::authority::*;
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

    let catalog = new_catalog();
    let addr = SocketAddr::from((LOCALHOST, self.port));
    let server = ServerFuture::new(catalog);
    let udp_socket = UdpSocket::bind(&addr).expect("error binding to UDP");
    let mut io_loop = Runtime::new().expect("error creating tokio runtime");

    let server_future: Box<Future<Item = (), Error = ()> + Send> =
      Box::new(future::lazy(move || {
        println!("binding UDP socket");
        server.register_socket(udp_socket);
        future::empty()
      }));
      io_loop.block_on(server_future).unwrap()
  }
}

fn new_catalog() -> Catalog {
  let dot_test_domain = create_dot_test_domain();
  let origin = dot_test_domain.origin().clone();
  let mut catalog = Catalog::new();
  catalog.upsert(origin, dot_test_domain);
  catalog
}

// Generate the ".test" domain data.
fn create_dot_test_domain() -> Authority {
  use std::collections::BTreeMap;
  use trust_dns::rr::rdata::SOA;
  use trust_dns::rr::*;

  let origin: Name = Name::from_str("test.").expect("error origin");

  let mut dot_test_records: Authority = Authority::new(
    origin.clone(),
    BTreeMap::new(),
    ZoneType::Master,
    false,
    false,
    false,
  );
  // SOA - not sure if we really need it.
  dot_test_records.upsert(
    Record::new()
      .set_name(origin.clone())
      .set_ttl(86400)
      .set_rr_type(RecordType::SOA)
      .set_dns_class(DNSClass::IN)
      .set_rdata(RData::SOA(SOA::new(
        Name::from_str("duwop.test").unwrap(),
        Name::from_str("admin.duwop.test").unwrap(),
        2019041501,
        7200,
        3600,
        360000,
        86400,
      )))
      .clone(),
    0,
  );
  // NS, not sure if we need it either.
  dot_test_records.upsert(
    Record::new()
      .set_name(origin.clone())
      .set_ttl(86400)
      .set_rr_type(RecordType::NS)
      .set_dns_class(DNSClass::IN)
      .set_rdata(RData::NS(Name::from_str("ns1.duwop.test").unwrap()))
      .clone(),
    0,
  );
  // Finally, the single address
  dot_test_records.upsert(
    Record::new()
      .set_name(Name::from_str("*.test.").unwrap())
      .set_ttl(68400)
      .set_rr_type(RecordType::A)
      .set_dns_class(DNSClass::IN)
      .set_rdata(RData::A(LOCALHOST))
      .clone(),
    0,
  );
  dot_test_records
}
