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

/** `url` as plain text, `![](url)`, `<img src="url">` or `[img]url[/img]`. */
export function formatLink(format: LinkFormat, url: string): string {
  switch (format) {
    case "url":
      return url;
    case "markdown":
      return `![](${url})`;
    case "html":
      return `<img src="${escapeAttribute(url)}">`;
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
