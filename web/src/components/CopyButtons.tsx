import { useEffect, useState } from "react";
import { altText, copyText, formatLink, LINK_FORMATS, type LinkFormat } from "../links";

/**
 * One button per link format; shows "Copied" briefly after a click. The
 * Markdown and HTML snippets use the original filename as alt text.
 */
export function CopyButtons({ url, filename }: { url: string; filename?: string | null }) {
  const alt = altText(filename);
  const [copied, setCopied] = useState<LinkFormat | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(null), 1500);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const copy = async (format: LinkFormat) => {
    try {
      await copyText(formatLink(format, url, alt));
      setFailed(false);
      setCopied(format);
    } catch {
      setFailed(true);
    }
  };

  return (
    <div className="copy-buttons">
      {LINK_FORMATS.map(({ format, label }) => (
        <button
          key={format}
          type="button"
          className="button-small"
          title={formatLink(format, url, alt)}
          onClick={() => void copy(format)}
        >
          {copied === format ? "Copied" : label}
        </button>
      ))}
      {failed && <span className="error small">Copy failed; select the link instead.</span>}
    </div>
  );
}
