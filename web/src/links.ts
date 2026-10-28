// The snippets the copy buttons put on the clipboard for an Image's URL.

export type LinkFormat = "url" | "markdown" | "html" | "bbcode";

export const LINK_FORMATS: { format: LinkFormat; label: string }[] = [
  { format: "url", label: "URL" },
  { format: "markdown", label: "Markdown" },
  { format: "html", label: "HTML" },
  { format: "bbcode", label: "BBCode" },
];

function escapeAttribute(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}

function escapeMarkdownAlt(value: string): string {
  return value.replace(/[\\[\]]/g, "\\$&");
}

/**
 * Alt text from an Image's original filename: the name without its extension,
 * or "" when that says nothing (no name, or a browser's generic "image").
 */
export function altText(originalFilename: string | null | undefined): string {
  const name = (originalFilename ?? "").trim().replace(/\.[^./\\]+$/, "").trim();
  return name.toLowerCase() === "image" ? "" : name;
}

/** `url` as plain text, `![alt](url)`, `<img src="url" alt="alt">` or `[img]url[/img]`. */
export function formatLink(format: LinkFormat, url: string, alt = ""): string {
  switch (format) {
    case "url":
      return url;
    case "markdown":
      return `![${escapeMarkdownAlt(alt)}](${url})`;
    case "html":
      return alt
        ? `<img src="${escapeAttribute(url)}" alt="${escapeAttribute(alt)}">`
        : `<img src="${escapeAttribute(url)}">`;
    case "bbcode":
      return `[img]${url}[/img]`;
  }
}

/** Copies text, falling back to a hidden textarea where the Clipboard API is unavailable (plain http). */
export async function copyText(text: string): Promise<void> {
  if (navigator.clipboard && window.isSecureContext) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  try {
    if (!document.execCommand("copy")) throw new Error("copy was refused");
  } finally {
    area.remove();
  }
}
