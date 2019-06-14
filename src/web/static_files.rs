use std::path::{Path, PathBuf};

use super::errors::*;

use failure::Error;
use futures::{future::Either, Future};
use http::status::StatusCode;
use hyper::{header, Body, Request, Response};
use log::{debug, warn};

// The function that returns a future of http responses for each hyper Request
// that is received. Errors are turned into an Error response (404 or 500).
pub(super) fn serve(
    req: Request<Body>,
    root_dir: &PathBuf,
) -> impl Future<Item = Response<Body>, Error = Error> {
    let uri_path = req.uri().path();
    if let Some(path) = local_path_for_request(&uri_path, root_dir) {
        Either::A(tokio_fs::file::File::open(path.clone()).then(
            move |open_result| match open_result {
                Ok(file) => Either::A(read_file(file, path)),
                // Since local_path_for_request verifies that the file exists
                // any other io::error is internal server error.
                Err(err) => Either::B(internal_server_error(Error::from(err))),
            },
        ))
    } else {
        Either::B(handle_404())
    }
}

// Read the file completely and construct a 200 response with that file as
// the body of the response.
fn read_file(
    file: tokio_fs::File,
    path: PathBuf,
) -> impl Future<Item = Response<Body>, Error = Error> {
    let buf: Vec<u8> = Vec::new();
    tokio_io::io::read_to_end(file, buf)
        .map_err(Error::from)
        .and_then(move |(_, buf)| {
            let mime_type = file_path_mime(&path);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_LENGTH, buf.len() as u64)
                .header(header::CONTENT_TYPE, mime_type.as_ref())
                .body(Body::from(buf))
                .map_err(Error::from)
        })
}

fn file_path_mime(file_path: &Path) -> mime::Mime {
    match file_path.extension().and_then(std::ffi::OsStr::to_str) {
        Some("html") => mime::TEXT_HTML,
        Some("css") => mime::TEXT_CSS,
        Some("js") => mime::TEXT_JAVASCRIPT,
        Some("jpg") => mime::IMAGE_JPEG,
        Some("png") => mime::IMAGE_PNG,
        Some("svg") => mime::IMAGE_SVG,
        Some("wasm") => "application/wasm".parse::<mime::Mime>().unwrap(),
        _ => mime::TEXT_PLAIN,
    }
}

/// returns the path for the requested file relative to the root dir after
/// normalization.
///
/// Returns None if:
/// * file doesn't exist
/// * file attempts to go above root (traversal directory attack)
///
/// If directory is requested returns `<DIR>/index.html` if it exists or None.
/// This might change in the future.
fn local_path_for_request(request_path: &str, root_dir: &Path) -> Option<PathBuf> {
    // This was in the original code and it seems like a good idea (although we're
    // doing extra check to prevent traversal directory attack later)
    if !request_path.starts_with('/') {
        return None;
    }
    // Trim off the url parameters
    let end = request_path.find('?').unwrap_or_else(|| request_path.len());
    let request_path = &request_path[0..end];

    let mut path = root_dir.to_owned();
    path.push(&request_path[1..]);

    // Maybe turn directory requests into index.html requests
    // TODO: maybe later allow directory listing?
    if request_path.ends_with('/') {
        path.push("index.html");
    }

    match path.canonicalize() {
        Ok(pth) => {
            debug!("canonicalized path: {:?}", &pth);
            if !pth.starts_with(root_dir) {
                warn!("Traversal attack: {:?}", &pth);
                None
            } else {
                Some(pth)
            }
        }
        Err(e) => {
            debug!("error normalizing path ({:?}): {}", path, e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    macro_rules! path_tests {
        ($name:ident, $request_path:expr, $root_dir:expr, $test_fn:expr) => {
            #[test]
            fn $name() {
                let file_path = local_path_for_request($request_path, $root_dir);
                $test_fn(file_path);
            }
        };
    }

    path_tests! {
      local_path_for_request_joins_root_with_path,
      "/Cargo.toml",
      &std::env::current_dir().unwrap(),
      |pth: Option<PathBuf>| {
        let mut match_to = std::env::current_dir().unwrap();
        match_to.push("Cargo.toml");
        assert_eq!(pth.unwrap(), match_to);
      }
    }

    path_tests! {
      local_path_for_request_does_not_return_non_existing_paths,
      "/index.html",
      &PathBuf::from("/no/such/directory"),
      |pth: Option<PathBuf>| {
        assert!(pth.is_none());
      }
    }

    path_tests! {
      local_path_for_request_does_not_accept_relative_uri_paths,
      "Cargo.toml",
      &std::env::current_dir().unwrap(),
      |pth: Option<PathBuf>| {
        assert_eq!(pth, None);
      }
    }

    path_tests! {
      local_path_for_request_normalizes_paths,
      "/src/../Cargo.toml",
      &std::env::current_dir().unwrap(),
      |pth: Option<PathBuf>| {
        let mut match_to = std::env::current_dir().unwrap();
        match_to.push("Cargo.toml");
        assert_eq!(pth, Some(match_to));
      }
    }

    path_tests! {
      local_path_for_request_does_not_allow_to_go_above_root,
      "/web/../../Cargo.toml",
      {
        let mut cd = std::env::current_dir().unwrap();
        cd.push("src/");
        &cd.clone()
      },
      |pth: Option<PathBuf>| {
        assert_eq!(pth, None);
      }
    }
}
