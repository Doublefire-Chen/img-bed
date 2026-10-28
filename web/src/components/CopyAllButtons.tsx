import { useEffect, useState } from "react";
import { copyText, formatLinks, LINK_FORMATS, type LinkFormat, type LinkItem } from "../links";

/** How many lines the tooltip previews before summing up the rest. */
const PREVIEW_LINES = 3;

function preview(text: string): string {
  const lines = text.split("\n");
  if (lines.length <= PREVIEW_LINES) return text;
  return [...lines.slice(0, PREVIEW_LINES), `…and ${lines.length - PREVIEW_LINES} more`].join("\n");
}

/**
 * Like CopyButtons, but for several Images at once: each format button copies
 * one snippet per Image, one per line, in the order given.
 */
export function CopyAllButtons({ label, items }: { label: string; items: readonly LinkItem[] }) {
  const [copied, setCopied] = useState<LinkFormat | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(null), 1500);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const copy = async (format: LinkFormat) => {
    try {
      await copyText(formatLinks(format, items));
      setFailed(false);
      setCopied(format);
    } catch {
      setFailed(true);
    }
  };

  return (
    <div className="copy-buttons copy-all">
      <span className="muted small">{label}</span>
      {LINK_FORMATS.map(({ format, label }) => (
        <button
          key={format}
          type="button"
          className="button-small"
          disabled={items.length === 0}
          title={preview(formatLinks(format, items))}
          onClick={() => void copy(format)}
        >
          {copied === format ? "Copied" : label}
        </button>
      ))}
      {failed && <span className="error small">Copy failed; copy the links one by one instead.</span>}
    </div>
  );
}
