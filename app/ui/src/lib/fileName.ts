/** File extensions for common response content types (without parameters like `charset`). */
const EXTENSIONS: Record<string, string> = {
  "application/json": "json",
  "application/ld+json": "jsonld",
  "application/problem+json": "json",
  "application/xml": "xml",
  "application/problem+xml": "xml",
  "application/yaml": "yaml",
  "application/x-yaml": "yaml",
  "application/javascript": "js",
  "application/pdf": "pdf",
  "application/zip": "zip",
  "application/gzip": "gz",
  "application/x-tar": "tar",
  "application/octet-stream": "bin",
  "application/x-www-form-urlencoded": "txt",
  "application/vnd.ms-excel": "xls",
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet": "xlsx",
  "application/vnd.openxmlformats-officedocument.wordprocessingml.document": "docx",
  "text/html": "html",
  "text/plain": "txt",
  "text/csv": "csv",
  "text/css": "css",
  "text/javascript": "js",
  "text/markdown": "md",
  "text/xml": "xml",
  "text/yaml": "yaml",
  "image/png": "png",
  "image/jpeg": "jpg",
  "image/gif": "gif",
  "image/webp": "webp",
  "image/svg+xml": "svg",
  "image/avif": "avif",
  "image/x-icon": "ico",
  "image/vnd.microsoft.icon": "ico",
  "audio/mpeg": "mp3",
  "video/mp4": "mp4",
};

/** The extension for a `Content-Type` value, e.g. `json` for `application/vnd.api+json`. */
export function extensionFor(contentType: string | null): string | null {
  const type = contentType?.split(";")[0]?.trim().toLowerCase();
  if (!type) return null;
  const known = EXTENSIONS[type];
  if (known) return known;
  // Structured syntax suffixes (RFC 6839): `application/vnd.github+json` is JSON.
  const suffix = type.match(/\+(json|xml|yaml|zip)$/)?.[1];
  if (suffix) return suffix;
  if (type.startsWith("text/")) return "txt";
  return null;
}

/** The `filename` of a `Content-Disposition` header, if it names one. */
function dispositionName(disposition: string | undefined): string | null {
  if (!disposition) return null;
  // `filename*=UTF-8''na%20me.pdf` (RFC 6266) wins over the plain `filename="name.pdf"`.
  const extended = disposition.match(/filename\*\s*=\s*[^']*'[^']*'([^;]+)/i)?.[1];
  if (extended) {
    try {
      return decodeURIComponent(extended.trim());
    } catch {
      // Malformed percent-encoding: fall back to the plain name.
    }
  }
  const plain = disposition.match(/filename\s*=\s*("([^"]*)"|[^;]+)/i);
  const name = (plain?.[2] ?? plain?.[1])?.trim();
  return name || null;
}

/**
 * A file name to suggest when saving a response body: the server's `Content-Disposition` name,
 * else `response.<ext>` from the content type (just `response` for unknown types).
 */
export function suggestedFileName(
  contentType: string | null,
  headers: { name: string; value?: string }[],
): string {
  const disposition = headers.find((h) => h.name.toLowerCase() === "content-disposition");
  // Only the last path segment: a server must not choose the folder.
  const fromServer = dispositionName(disposition?.value)?.split(/[/\\]/).pop();
  if (fromServer) return fromServer;
  const ext = extensionFor(contentType);
  return ext ? `response.${ext}` : "response";
}
