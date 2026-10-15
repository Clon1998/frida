//! Generating code from requests (PLAN.md §11.3). Only curl for now.

mod curl;

pub use curl::{CurlBody, CurlOptions, CurlPart, CurlRequest, Shell, curl_request, render_curl};
