//! HTTP execution (PLAN.md §7): prepare a request, send it, and present the response.

mod prepare;
mod send;
mod view;

pub use prepare::{
    DEFAULT_TIMEOUT, PreparedBody, PreparedPart, PreparedRequest, RequestContext, prepare,
};
pub use send::{HttpResponse, SendError, send};
pub use view::{MAX_DISPLAY_BYTES, ResponseCache, ResponseView};
