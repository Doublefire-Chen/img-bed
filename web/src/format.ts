// Formatting shared by the admin pages.

/** A byte count like "512 B", "12.3 KB", "4.5 MB" or "1.2 GB". */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/** A date like "27 October 2026", in the browser's locale. */
export function formatDate(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

export function formatDateTime(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString();
}

/** The Path's address on this origin, for previews (the public URL may be another host in development). */
export function previewSrc(url: string): string {
  try {
    return new URL(url).pathname;
  } catch {
    return url;
  }
}
