import { useQueryClient } from "@tanstack/react-query";
import { type ChangeEvent, type DragEvent, useCallback, useEffect, useRef, useState } from "react";
import { ApiError, api, LIBRARY_KEY, type Uploaded, isUnauthorized } from "../api";
import { CopyButtons } from "../components/CopyButtons";
import { formatDate, formatDateTime, formatSize, previewSrc } from "../format";
import { SESSION_KEY } from "../session";

/** How many files are sent at once. */
const PARALLEL_UPLOADS = 3;

type Item =
  | { key: number; name: string; size: number; status: "waiting" | "uploading" }
  | { key: number; name: string; size: number; status: "done"; result: Uploaded }
  | { key: number; name: string; size: number; status: "error"; message: string };

function describe(error: unknown): string {
  if (error instanceof ApiError) {
    switch (error.code) {
      case "unsupported_type":
        return "Not a supported image. Accepted types are JPEG, PNG, GIF, WebP, AVIF and SVG.";
      case "too_large":
        return error.message.replace(/^file is larger/, "Larger") + ".";
    }
    return error.message;
  }
  return "Could not reach the server.";
}

export function UploadPage() {
  const queryClient = useQueryClient();
  const [items, setItems] = useState<Item[]>([]);
  const [dragging, setDragging] = useState(false);
  const queue = useRef<{ key: number; file: File }[]>([]);
  const running = useRef(0);
  const nextKey = useRef(1);
  const input = useRef<HTMLInputElement>(null);

  const update = (key: number, item: Item) =>
    setItems((current) => current.map((i) => (i.key === key ? item : i)));

  const pump = useCallback(() => {
    while (running.current < PARALLEL_UPLOADS && queue.current.length > 0) {
      const { key, file } = queue.current.shift()!;
      running.current += 1;
      const base = { key, name: file.name, size: file.size };
      update(key, { ...base, status: "uploading" });
      api
        .uploadFile(file)
        .then((result) => {
          update(key, { ...base, status: "done", result });
          void queryClient.invalidateQueries({ queryKey: LIBRARY_KEY });
        })
        .catch((error: unknown) => {
          if (isUnauthorized(error)) queryClient.setQueryData(SESSION_KEY, null);
          update(key, { ...base, status: "error", message: describe(error) });
        })
        .finally(() => {
          running.current -= 1;
          pump();
        });
    }
  }, [queryClient]);

  const addFiles = useCallback(
    (files: Iterable<File>) => {
      const added: Item[] = [];
      for (const file of files) {
        const key = nextKey.current++;
        queue.current.push({ key, file });
        added.push({ key, name: file.name || "pasted image", size: file.size, status: "waiting" });
      }
      if (added.length === 0) return;
      // Newest first, so fresh results appear at the top.
      setItems((current) => [...added.reverse(), ...current]);
      pump();
    },
    [pump],
  );

  // Paste anywhere on the page.
  useEffect(() => {
    const onPaste = (event: ClipboardEvent) => {
      const files = Array.from(event.clipboardData?.files ?? []);
      if (files.length === 0) return;
      event.preventDefault();
      addFiles(files);
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [addFiles]);

  // Dropping anywhere on the page uploads instead of opening the file.
  useEffect(() => {
    const prevent = (event: globalThis.DragEvent) => {
      if (event.dataTransfer?.types.includes("Files")) event.preventDefault();
    };
    const onDrop = (event: globalThis.DragEvent) => {
      if (!event.dataTransfer?.files.length) return;
      event.preventDefault();
      setDragging(false);
      addFiles(Array.from(event.dataTransfer.files));
    };
    window.addEventListener("dragover", prevent);
    window.addEventListener("drop", onDrop);
    return () => {
      window.removeEventListener("dragover", prevent);
      window.removeEventListener("drop", onDrop);
    };
  }, [addFiles]);

  const onDragOver = (event: DragEvent) => {
    if (event.dataTransfer.types.includes("Files")) setDragging(true);
  };

  const onChoose = (event: ChangeEvent<HTMLInputElement>) => {
    addFiles(Array.from(event.target.files ?? []));
    event.target.value = "";
  };

  const finished = items.filter((i) => i.status === "done" || i.status === "error").length;

  return (
    <section>
      <h1>Upload</h1>
      <div
        className={`dropzone${dragging ? " dropzone-active" : ""}`}
        onDragOver={onDragOver}
        onDragLeave={() => setDragging(false)}
        onClick={() => input.current?.click()}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            input.current?.click();
          }
        }}
        role="button"
        tabIndex={0}
      >
        <p>
          <strong>Drop images here</strong>, paste from the clipboard, or click to choose files.
        </p>
        <p className="muted small">JPEG, PNG, GIF, WebP, AVIF or SVG. Several at once is fine.</p>
      </div>
      {/* Outside the drop zone, so its click does not bubble back into it. */}
      <input
        ref={input}
        type="file"
        multiple
        accept="image/jpeg,image/png,image/gif,image/webp,image/avif,image/svg+xml"
        onChange={onChoose}
        hidden
      />

      {items.length > 0 && (
        <>
          <div className="results-header">
            <p className="muted small">
              {finished} of {items.length} finished
            </p>
            {finished === items.length && (
              <button type="button" className="button-quiet button-small" onClick={() => setItems([])}>
                Clear
              </button>
            )}
          </div>
          <ul className="results">
            {items.map((item) => (
              <li
                key={item.key}
                className={`result result-${item.status}${
                  item.status === "done" && item.result.duplicate ? " result-duplicate" : ""
                }`}
              >
                {item.status === "done" ? (
                  <img className="thumb" src={previewSrc(item.result.image.url)} alt="" loading="lazy" />
                ) : (
                  <div className="thumb thumb-empty" aria-hidden="true" />
                )}
                <div className="result-body">
                  <p className="result-name">
                    {item.name} <span className="muted small">{formatSize(item.size)}</span>
                  </p>
                  {item.status === "waiting" && <p className="muted small">Waiting…</p>}
                  {item.status === "uploading" && <p className="muted small">Uploading…</p>}
                  {item.status === "error" && (
                    <p className="error small" role="alert">
                      {item.message}
                    </p>
                  )}
                  {item.status === "done" && <UploadedLinks name={item.name} result={item.result} />}
                </div>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

/**
 * The links of a stored file: its Path for a new Image, or, for a Duplicate
 * Upload, an "Already uploaded" notice and every existing Path (Primary first).
 */
function UploadedLinks({ name, result }: { name: string; result: Uploaded }) {
  const { image } = result;
  const paths = result.duplicate ? image.paths : image.paths.filter((p) => p.is_primary);
  const shown = paths.length > 0 ? paths : [{ path: image.path, url: image.url, is_primary: true }];
  const uploadedAt = result.original_uploaded_at ?? image.uploaded_at;
  return (
    <>
      {result.restored && (
        <p className="notice small" role="status">
          <strong>Restored from Trash.</strong> All {image.paths.length === 1 ? "its Path works" : `its ${image.paths.length} Paths work`} again.
        </p>
      )}
      {result.duplicate && (
        <p className="notice small" role="status">
          Already uploaded on <time title={formatDateTime(uploadedAt)}>{formatDate(uploadedAt)}</time>
          {image.original_filename && image.original_filename !== name && (
            <> as {image.original_filename}</>
          )}
          . Nothing new was created; use {shown.length === 1 ? "its existing Path" : "one of its existing Paths"}.
        </p>
      )}
      <ul className="path-links">
        {shown.map((p) => (
          <li key={p.path}>
            <p className="result-path">
              <code>/{p.path}</code>
              {result.duplicate && shown.length > 1 && p.is_primary && (
                <span className="badge small">Primary</span>
              )}
            </p>
            <input
              className="result-url"
              readOnly
              value={p.url}
              aria-label={`URL of ${name}${shown.length > 1 ? ` at /${p.path}` : ""}`}
              onFocus={(e) => e.target.select()}
            />
            <CopyButtons url={p.url} filename={image.original_filename} />
          </li>
        ))}
      </ul>
    </>
  );
}
