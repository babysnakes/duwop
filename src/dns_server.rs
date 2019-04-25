use futures::{future, Future};
use log::{debug, info};
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use trust_dns_server::authority::{Authority, Catalog, ZoneType};
use trust_dns_server::store::in_memory::InMemoryAuthority;
use trust_dns_server::ServerFuture;

const LOCALHOST: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);

pub struct DNSServer {
  pub port: u16,
  pub subdomains: Vec<String>,
}

impl DNSServer {
  pub fn run(&self) -> Box<Future<Item = (), Error = ()> + Send> {
    use tokio_udp::*;

    let addr = SocketAddr::from((LOCALHOST, self.port));
    let mut catalog = Catalog::new();
    info!("Adding '.info' domain");
    add_to_catalog(&mut catalog, "test.");
    for sub_domain in &self.subdomains {
      info!("Adding '{}' domain", &sub_domain);
      add_to_catalog(&mut catalog, &sub_domain);
    }
    let server = ServerFuture::new(catalog);
    let udp_socket = UdpSocket::bind(&addr).expect("error binding to UDP");

    Box::new(future::lazy(move || {
      info!("binding UDP socket");
      server.register_socket(udp_socket);
      future::empty()
    }))
  }
}

fn add_to_catalog(catalog: &mut Catalog, domain: &str) {
  let dot_test_domain = generate_domain(domain);
  let origin = dot_test_domain.origin().clone();
  catalog.upsert(origin, Box::new(dot_test_domain));
}

// Generate the ".test" domain data.
fn generate_domain(domain: &str) -> InMemoryAuthority {
  use trust_dns::rr::rdata::SOA;
  use trust_dns::rr::*;

  let origin: Name = Name::from_str(domain).expect("error origin");
  let mname = format!("duwop.{}", domain);
  let rname = format!("admin.duwop.{}", domain);
  let ns1 = format!("ns1.duwop.{}", domain);
  let wildcard = format!("*.{}", domain);

  let mut dot_test_records: InMemoryAuthority =
    InMemoryAuthority::empty(origin.clone(), ZoneType::Master, false);
  // SOA - not sure if we really need it.
  debug!("generating SOA record for {}", &domain);
  dot_test_records.upsert(
    Record::new()
      .set_name(origin.clone())
      .set_ttl(86400)
      .set_rr_type(RecordType::SOA)
      .set_dns_class(DNSClass::IN)
      .set_rdata(RData::SOA(SOA::new(
        Name::from_str(&mname).unwrap(),
        Name::from_str(&rname).unwrap(),
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
  debug!("generating NS record for {}", &domain);
  dot_test_records.upsert(
    Record::new()
      .set_name(origin.clone())
      .set_ttl(86400)
      .set_rr_type(RecordType::NS)
      .set_dns_class(DNSClass::IN)
      .set_rdata(RData::NS(Name::from_str(&ns1).unwrap()))
      .clone(),
    0,
  );
  // Finally, the single address
  debug!("generating wildcard record for {}", &domain);
  dot_test_records.upsert(
    Record::new()
      .set_name(Name::from_str(&wildcard).unwrap())
      .set_ttl(68400)
      .set_rr_type(RecordType::A)
      .set_dns_class(DNSClass::IN)
      .set_rdata(RData::A(LOCALHOST))
      .clone(),
    0,
  );
  dot_test_records
}
