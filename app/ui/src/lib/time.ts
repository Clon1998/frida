/** "just now", "5 min ago", "3 h ago", "2 d ago" or the date, for an ISO timestamp. */
export function timeAgo(iso: string, now: number = Date.now()): string {
  const seconds = Math.max(0, Math.round((now - Date.parse(iso)) / 1000));
  if (seconds < 45) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  if (days < 7) return `${days} d ago`;
  return iso.slice(0, 10);
}

/** "in 54 min", "in 2 h", "in 3 d" (or the date) for a future ISO timestamp. */
function timeUntil(iso: string, now: number): string {
  const minutes = Math.max(1, Math.round((Date.parse(iso) - now) / 60_000));
  if (minutes < 60) return `in ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `in ${hours} h`;
  const days = Math.round(hours / 24);
  if (days < 7) return `in ${days} d`;
  return `on ${iso.slice(0, 10)}`;
}

/** "expires in 54 min", "expired 3 min ago" or "no expiry", for an OAuth2 token. */
export function expiryLabel(expiresAt: string | null, now: number = Date.now()): string {
  if (expiresAt === null) return "no expiry";
  return Date.parse(expiresAt) > now
    ? `expires ${timeUntil(expiresAt, now)}`
    : `expired ${timeAgo(expiresAt, now)}`;
}
