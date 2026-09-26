// ABOUTME: Formats absolute timestamps with a relative "x ago" suffix.
// ABOUTME: Keeps admin dates unambiguous across locales without extra deps.
export function formatRelativeUpdatedAt(timestamp?: string | null, now: number = Date.now()): string {
  if (!timestamp) {
    return "Unknown";
  }
  const date = new Date(timestamp);
  const time = date.getTime();
  if (Number.isNaN(time)) {
    return "Unknown";
  }
  const absolute = date.toLocaleString();
  const diffMs = now - time;
  if (diffMs < 0 || diffMs < 60_000) {
    return `${absolute} (just now)`;
  }
  const minutes = Math.floor(diffMs / 60_000);
  if (minutes < 60) {
    return `${absolute} (${minutes}m ago)`;
  }
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return `${absolute} (${hours}h ago)`;
  }
  const days = Math.floor(hours / 24);
  if (days < 30) {
    return `${absolute} (${days}d ago)`;
  }
  const months = Math.floor(days / 30);
  if (months < 12) {
    return `${absolute} (${months}mo ago)`;
  }
  const years = Math.floor(months / 12);
  return `${absolute} (${years}y ago)`;
}
