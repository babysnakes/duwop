use super::app_defaults::*;

use std::path::PathBuf;

use failure::{format_err, Error, ResultExt};
use log::debug;
use openssl::asn1::{Asn1Time, Asn1TimeRef};
use openssl::bn::{BigNum, MsbOption};
use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, PKeyRef, Private};
use openssl::rsa::Rsa;
use openssl::x509::extension::{
    AuthorityKeyIdentifier, BasicConstraints, KeyUsage, SubjectAlternativeName,
    SubjectKeyIdentifier,
};
use openssl::x509::{X509NameBuilder, X509Ref, X509Req, X509ReqBuilder, X509};

/// Make a CA certificate and private key (taken from openssl example).
pub fn mk_ca_cert() -> Result<(X509, PKey<Private>), Error> {
    let rsa = Rsa::generate(2048)?;
    let privkey = PKey::from_rsa(rsa)?;

    let mut x509_name = X509NameBuilder::new()?;
    x509_name.append_entry_by_text("C", TLS_ENTRY_C)?;
    x509_name.append_entry_by_text("ST", TLS_ENTRY_ST)?;
    x509_name.append_entry_by_text("O", TLS_ENTRY_O)?;
    x509_name.append_entry_by_text("CN", TLS_ENTRY_CN)?;
    let x509_name = x509_name.build();

    let mut cert_builder = X509::builder()?;
    cert_builder.set_version(2)?;
    let serial_number = {
        let mut serial = BigNum::new()?;
        serial.rand(159, MsbOption::MAYBE_ZERO, false)?;
        serial.to_asn1_integer()?
    };
    cert_builder.set_serial_number(&serial_number)?;
    cert_builder.set_subject_name(&x509_name)?;
    cert_builder.set_issuer_name(&x509_name)?;
    cert_builder.set_pubkey(&privkey)?;
    let not_before = Asn1Time::days_from_now(100)?;
    cert_builder.set_not_before(&not_before)?;
    let not_after = Asn1Time::days_from_now(365)?;
    cert_builder.set_not_after(&not_after)?;

    cert_builder.append_extension(BasicConstraints::new().critical().ca().build()?)?;
    cert_builder.append_extension(
        KeyUsage::new()
            .critical()
            .key_cert_sign()
            .crl_sign()
            .build()?,
    )?;

    let subject_key_identifier =
        SubjectKeyIdentifier::new().build(&cert_builder.x509v3_context(None, None))?;
    cert_builder.append_extension(subject_key_identifier)?;

    cert_builder.sign(&privkey, MessageDigest::sha256())?;
    let cert = cert_builder.build();

    Ok((cert, privkey))
}

// Load CA certificate and key from files (actually, its good from any
// certificate and key but in our context we only load CA keys).
pub fn load_ca_cert(key_file: PathBuf, cert_file: PathBuf) -> Result<(X509, PKey<Private>), Error> {
    debug!("loading key ({:?}) and cert ({:?})", &key_file, &cert_file);
    let key_bytes = std::fs::read(key_file)?;
    let key = PKey::private_key_from_pem(&key_bytes[..])?;
    let cert_bytes = std::fs::read(cert_file)?;
    let cert = X509::from_pem(&cert_bytes[..])?;
    Ok((cert, key))
}

/// Make a certificate and private key signed by the given CA cert and private key
pub fn mk_ca_signed_cert(
    ca_cert: &X509Ref,
    ca_privkey: &PKeyRef<Private>,
    dns_names_for_san: Vec<String>,
) -> Result<(X509, PKey<Private>), Error> {
    let rsa = Rsa::generate(2048)?;
    let privkey = PKey::from_rsa(rsa)?;

    let req = mk_request(&privkey).context("creating certificate request")?;

    let mut cert_builder = X509::builder()?;
    cert_builder.set_version(2)?;
    let serial_number = {
        let mut serial = BigNum::new()?;
        serial.rand(159, MsbOption::MAYBE_ZERO, false)?;
        serial.to_asn1_integer()?
    };
    cert_builder.set_serial_number(&serial_number)?;
    cert_builder.set_subject_name(req.subject_name())?;
    cert_builder.set_issuer_name(ca_cert.subject_name())?;
    cert_builder.set_pubkey(&privkey)?;
    let not_before = Asn1Time::days_from_now(0)?;
    cert_builder.set_not_before(&not_before)?;
    let not_after = Asn1Time::days_from_now(365)?;
    cert_builder.set_not_after(&not_after)?;

    cert_builder.append_extension(BasicConstraints::new().build()?)?;

    cert_builder.append_extension(
        KeyUsage::new()
            .critical()
            .non_repudiation()
            .digital_signature()
            .key_encipherment()
            .build()?,
    )?;

    let subject_key_identifier =
        SubjectKeyIdentifier::new().build(&cert_builder.x509v3_context(Some(ca_cert), None))?;
    cert_builder.append_extension(subject_key_identifier)?;

    let auth_key_identifier = AuthorityKeyIdentifier::new()
        .keyid(false)
        .issuer(false)
        .build(&cert_builder.x509v3_context(Some(ca_cert), None))?;
    cert_builder.append_extension(auth_key_identifier)?;

    let subject_alt_name = {
        let mut builder = SubjectAlternativeName::new();
        if dns_names_for_san.is_empty() {
            builder.dns("duwop.test");
        } else {
            for name in dns_names_for_san {
                builder.dns(&format!("{}.test", &name));
                builder.dns(&format!("*.{}.test", &name));
            }
        }
        builder.build(&cert_builder.x509v3_context(Some(ca_cert), None))?
    };
    cert_builder.append_extension(subject_alt_name)?;

    cert_builder.sign(&ca_privkey, MessageDigest::sha256())?;
    let cert = cert_builder.build();

    Ok((cert, privkey))
}

/// Verifies that the supplied certificate is not expired now or in the next
/// Make a X509 request with the given private key
fn mk_request(privkey: &PKey<Private>) -> Result<X509Req, Error> {
    let mut req_builder = X509ReqBuilder::new()?;
    req_builder.set_pubkey(&privkey)?;

    let mut x509_name = X509NameBuilder::new()?;
    x509_name.append_entry_by_text("C", TLS_ENTRY_C)?;
    x509_name.append_entry_by_text("ST", TLS_ENTRY_ST)?;
    x509_name.append_entry_by_text("O", TLS_ENTRY_O)?;
    x509_name.append_entry_by_text("CN", TLS_ENTRY_CN)?;
    let x509_name = x509_name.build();
    req_builder.set_subject_name(&x509_name)?;

    req_builder.sign(&privkey, MessageDigest::sha256())?;
    let req = req_builder.build();
    Ok(req)
}

/// `min_days` days.
pub fn validate_ca(cert: X509, min_days: u32) -> Result<bool, Error> {
    // This proved to be much harder then it should. Took me a while to find a
    // way to do it in rust without including another full-feature ssl library.
    // Finally found a hacky solution here:
    // https://ayende.com/blog/185764-A/using-tls-with-rust-authentication
    use foreign_types::ForeignTypeRef;

    extern "C" {
        fn ASN1_TIME_diff(
            pday: *mut std::os::raw::c_int,
            psec: *mut std::os::raw::c_int,
            from: *const openssl_sys::ASN1_TIME,
            to: *const openssl_sys::ASN1_TIME,
        ) -> std::os::raw::c_int;
    }

    fn is_before(a: &Asn1TimeRef, b: &Asn1TimeRef) -> Result<bool, Error> {
        unsafe {
            let mut day: std::os::raw::c_int = 0;
            let mut sec: std::os::raw::c_int = 0;
            match ASN1_TIME_diff(&mut day, &mut sec, a.as_ptr(), b.as_ptr()) {
                0 => Err(format_err!("Error comparing asn time")),
                _ => Ok(day > 0 || sec > 0),
            }
        }
    }
    let min = Asn1Time::days_from_now(min_days)?;
    if is_before(cert.not_after(), &min)? {
        Ok(false)
    } else {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mk_ca_signed_cert_with_empty_san() {
        let (cert, key) = mk_ca_cert().unwrap();
        let (cert, _key) = mk_ca_signed_cert(&cert, &key, vec![]).unwrap();
        assert_eq!(
            cert.subject_alt_names()
                .unwrap()
                .get(0)
                .unwrap()
                .dnsname()
                .unwrap(),
            "duwop.test"
        );
    }

    #[test]
    fn test_mk_ca_signed_cert_with_names() {
        let (cert, key) = mk_ca_cert().unwrap();
        let (cert, _key) =
            mk_ca_signed_cert(&cert, &key, vec!["hello".into(), "world".into()]).unwrap();
        assert_eq!(cert.subject_alt_names().unwrap().len(), 4);
    }
}
