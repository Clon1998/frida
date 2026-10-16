//! Decompressing response bodies. reqwest can do this itself, but it then removes the
//! `Content-Encoding` and `Content-Length` headers from the response, so the headers view and
//! the console would not show what the server really sent (Postman shows them). So the main
//! send turns reqwest's decompression off and calls [`decode`] instead.

use std::io::Read;

/// Undoes the `Content-Encoding` of a body. Several encodings (`gzip, br`) were applied in
/// order, so they are undone in reverse. Unknown encodings, and bodies that fail to decode, are
/// returned unchanged: showing the raw bytes is more useful than failing the whole request.
pub(crate) fn decode(content_encoding: Option<&str>, body: Vec<u8>) -> Vec<u8> {
    let Some(header) = content_encoding else {
        return body;
    };
    let encodings: Vec<String> = header
        .split(',')
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| !e.is_empty() && e != "identity")
        .collect();
    let mut body = body;
    for encoding in encodings.iter().rev() {
        match decode_one(encoding, &body) {
            Some(Ok(decoded)) => body = decoded,
            Some(Err(err)) => {
                tracing::debug!(encoding, %err, "could not decode response body");
                return body;
            }
            None => {
                tracing::debug!(encoding, "unknown content encoding, body left as is");
                return body;
            }
        }
    }
    body
}

/// `None` for an encoding we don't know.
fn decode_one(encoding: &str, body: &[u8]) -> Option<std::io::Result<Vec<u8>>> {
    let mut out = Vec::new();
    // `&[u8]` implements `Read` (like a `ByteArrayInputStream`), so each decoder wraps it.
    let result = match encoding {
        "gzip" | "x-gzip" => flate2::read::MultiGzDecoder::new(body).read_to_end(&mut out),
        // "deflate" should be zlib-wrapped, but some servers send raw deflate; try both.
        "deflate" => flate2::read::ZlibDecoder::new(body)
            .read_to_end(&mut out)
            .or_else(|_| {
                out.clear();
                flate2::read::DeflateDecoder::new(body).read_to_end(&mut out)
            }),
        "br" => brotli::Decompressor::new(body, 4096).read_to_end(&mut out),
        "zstd" => zstd::stream::read::Decoder::new(body)
            .and_then(|mut decoder| decoder.read_to_end(&mut out)),
        _ => return None,
    };
    Some(result.map(|_| out))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const TEXT: &[u8] = b"{\"hello\":\"world\"}";

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn decodes_each_encoding() {
        assert_eq!(decode(Some("gzip"), gzip(TEXT)), TEXT);

        let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        zlib.write_all(TEXT).unwrap();
        assert_eq!(decode(Some("deflate"), zlib.finish().unwrap()), TEXT);

        let mut raw = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
        raw.write_all(TEXT).unwrap();
        assert_eq!(decode(Some("deflate"), raw.finish().unwrap()), TEXT);

        let mut br = Vec::new();
        brotli::CompressorWriter::new(&mut br, 4096, 5, 22)
            .write_all(TEXT)
            .unwrap();
        assert_eq!(decode(Some("br"), br), TEXT);

        let zstd = zstd::stream::encode_all(TEXT, 3).unwrap();
        assert_eq!(decode(Some("zstd"), zstd), TEXT);
    }

    #[test]
    fn undoes_several_encodings_in_reverse() {
        assert_eq!(decode(Some("gzip, gzip"), gzip(&gzip(TEXT))), TEXT);
    }

    #[test]
    fn leaves_unknown_and_broken_bodies_alone() {
        assert_eq!(decode(None, TEXT.to_vec()), TEXT);
        assert_eq!(decode(Some("identity"), TEXT.to_vec()), TEXT);
        assert_eq!(decode(Some("compress"), TEXT.to_vec()), TEXT);
        assert_eq!(decode(Some("gzip"), TEXT.to_vec()), TEXT);
    }
}
