import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useEffect, useRef, useState } from "react";
import { Link, useLocation, useParams } from "react-router";
import { ApiError, api, type Image, type ImagePath, LIBRARY_KEY } from "../api";
import { CopyButtons } from "../components/CopyButtons";
import { formatDateTime, formatSize, previewSrc } from "../format";

function imageKey(id: number) {
  return [...LIBRARY_KEY, "image", id] as const;
}

/** Everything about one Image: preview, facts and all its Paths. */
export function ImagePage() {
  const { id } = useParams();
  const location = useLocation();
  // The Library page and search we came from, if any.
  const from = (location.state as { librarySearch?: string } | null)?.librarySearch ?? "";
  const imageId = Number(id);
  const valid = Number.isSafeInteger(imageId) && imageId > 0;

  const query = useQuery({
    queryKey: imageKey(imageId),
    queryFn: () => api.image(imageId),
    enabled: valid,
  });

  const back = (
    <p className="small">
      <Link to={`/library${from}`}>← Library</Link>
    </p>
  );

  const notFound = !valid || (query.error instanceof ApiError && query.error.status === 404);
  if (notFound) {
    return (
      <section>
        {back}
        <div className="empty">
          <p>There is no Image {id}.</p>
        </div>
      </section>
    );
  }
  if (query.isError) {
    return (
      <section>
        {back}
        <p className="error" role="alert">
          Could not load the Image: {query.error.message}
        </p>
      </section>
    );
  }
  if (!query.data) {
    return (
      <section>
        {back}
        <p className="muted">Loading…</p>
      </section>
    );
  }
  return (
    <section>
      {back}
      <ImageDetail image={query.data} />
    </section>
  );
}

function ImageDetail({ image }: { image: Image }) {
  const dimensions =
    image.width != null && image.height != null ? `${image.width} × ${image.height}` : "Unknown";
  return (
    <div className="detail">
      <a className="detail-preview" href={previewSrc(image.url)} target="_blank" rel="noreferrer">
        <img src={previewSrc(image.url)} alt={image.original_filename} />
      </a>

      <div className="detail-body">
        <h1 className="detail-title">{image.original_filename || `Image ${image.id}`}</h1>
        {image.trashed_at != null && (
          <p className="notice small">In the Trash since {formatDateTime(image.trashed_at)}.</p>
        )}
        <dl className="facts">
          <dt>Size</dt>
          <dd>
            {formatSize(image.size)} <span className="muted small">({image.size.toLocaleString()} bytes)</span>
          </dd>
          <dt>Dimensions</dt>
          <dd>{dimensions}</dd>
          <dt>Type</dt>
          <dd>{image.type}</dd>
          <dt>Uploaded</dt>
          <dd>{formatDateTime(image.uploaded_at)}</dd>
          <dt>Original filename</dt>
          <dd className="wrap">{image.original_filename || "—"}</dd>
          <dt>SHA-256</dt>
          <dd>
            <code className="hash">{image.sha256}</code>
          </dd>
        </dl>

        <PathsSection image={image} />
      </div>
    </div>
  );
}

/**
 * After a Path change: show the Image the server answered with, and refetch
 * the rest of the Library (Primary Paths and search results may have moved).
 */
function useImageUpdate() {
  const queryClient = useQueryClient();
  return (image: Image) => {
    queryClient.setQueryData(imageKey(image.id), image);
    void queryClient.invalidateQueries({
      queryKey: LIBRARY_KEY,
      predicate: (q) => q.queryKey[1] !== "image" || q.queryKey[2] !== image.id,
    });
  };
}

/** The Image's Paths, Primary first, with copy buttons and Path management. */
function PathsSection({ image }: { image: Image }) {
  const [adding, setAdding] = useState(false);
  const onlyOne = image.paths.length === 1;
  return (
    <section className="detail-paths" aria-labelledby="paths-heading">
      <div className="section-header">
        <h2 id="paths-heading">
          Paths <span className="muted small">({image.paths.length})</span>
        </h2>
        {!adding && (
          <button type="button" className="button-small" onClick={() => setAdding(true)}>
            Add Path
          </button>
        )}
      </div>
      {adding && <AddPathForm image={image} onDone={() => setAdding(false)} />}
      <ul className="path-links">
        {image.paths.map((p) => (
          <PathRow key={p.path} imageId={image.id} path={p} onlyOne={onlyOne} />
        ))}
      </ul>
    </section>
  );
}

/**
 * What the Owner typed, as a Path: surrounding blanks and the leading "/"
 * shown before the field are dropped. Everything else is checked by the server.
 */
function cleanInput(input: string): string {
  return input.trim().replace(/^\/+/, "");
}

function AddPathForm({ image, onDone }: { image: Image; onDone: () => void }) {
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const update = useImageUpdate();
  const mutation = useMutation({
    mutationFn: (path: string) => api.addPath(image.id, path),
    onSuccess: (updated) => {
      update(updated);
      onDone();
    },
  });

  useEffect(() => inputRef.current?.focus(), []);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const path = cleanInput(value);
    if (path) mutation.mutate(path);
  };

  return (
    <form
      className="add-path"
      onSubmit={submit}
      onKeyDown={(e) => {
        if (e.key === "Escape") onDone();
      }}
    >
      <div className="add-path-row">
        <label className="path-input">
          <span className="muted" aria-hidden="true">
            /
          </span>
          <input
            ref={inputRef}
            value={value}
            onChange={(e) => {
              setValue(e.target.value);
              mutation.reset();
            }}
            placeholder="cat.png"
            aria-label="New Path"
            aria-invalid={mutation.isError}
            aria-describedby="add-path-help"
            spellCheck={false}
            autoCapitalize="off"
            autoComplete="off"
          />
        </label>
        <button
          type="submit"
          className="button-primary button-small"
          disabled={mutation.isPending || !cleanInput(value)}
        >
          {mutation.isPending ? "Adding…" : "Add"}
        </button>
        <button type="button" className="button-small button-quiet" onClick={onDone}>
          Cancel
        </button>
      </div>
      {mutation.isError ? (
        <p className="error small" role="alert" id="add-path-help">
          {addPathError(mutation.error)}
        </p>
      ) : (
        <p className="muted small" id="add-path-help">
          Letters and digits of any language, and - _ . ~ /. It serves this Image at once; the other
          Paths keep working.
        </p>
      )}
    </form>
  );
}

function addPathError(error: Error): string {
  if (error instanceof ApiError) {
    switch (error.code) {
      case "path_taken":
        return error.message.includes("another Image")
          ? `${error.message}. Remove it from that Image first to move it here.`
          : `${error.message}.`;
      case "reserved_path":
      case "invalid_path":
        return capitalize(error.message) + ".";
    }
  }
  return `Could not add the Path: ${error.message}`;
}

function capitalize(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function PathRow({
  imageId,
  path,
  onlyOne,
}: {
  imageId: number;
  path: ImagePath;
  onlyOne: boolean;
}) {
  const [confirming, setConfirming] = useState(false);
  const update = useImageUpdate();
  const primary = useMutation({
    mutationFn: () => api.setPrimary(imageId, path.path),
    onSuccess: update,
  });
  const remove = useMutation({
    mutationFn: () => api.removePath(imageId, path.path),
    onSuccess: update,
  });
  const busy = primary.isPending || remove.isPending;
  const error = primary.error ?? remove.error;

  return (
    <li className={path.is_primary ? "path-primary" : undefined}>
      <div className="path-row-header">
        <p className="result-path">
          <code>/{path.path}</code>
          {path.is_primary && <span className="badge small">Primary</span>}
        </p>
        {!confirming && (
          <div className="path-actions">
            {!path.is_primary && (
              <button
                type="button"
                className="button-small"
                disabled={busy}
                onClick={() => {
                  remove.reset();
                  primary.mutate();
                }}
              >
                Set Primary
              </button>
            )}
            <button
              type="button"
              className="button-small button-quiet button-danger"
              disabled={busy || onlyOne}
              title={onlyOne ? "An Image always keeps at least one Path." : undefined}
              onClick={() => {
                primary.reset();
                remove.reset();
                setConfirming(true);
              }}
            >
              Remove
            </button>
          </div>
        )}
      </div>
      {confirming && (
        <div className="confirm" role="alertdialog" aria-label={`Remove /${path.path}`}>
          <p className="small">
            Remove <code>/{path.path}</code>? Every link using it stops working, and the Path
            becomes free for any Image.
            {path.is_primary && " The oldest remaining Path becomes Primary."}
          </p>
          <div className="copy-buttons">
            <button
              type="button"
              className="button-small button-danger-solid"
              disabled={busy}
              autoFocus
              onClick={() => remove.mutate(undefined, { onSettled: () => setConfirming(false) })}
            >
              {remove.isPending ? "Removing…" : "Remove Path"}
            </button>
            <button
              type="button"
              className="button-small button-quiet"
              disabled={busy}
              onClick={() => setConfirming(false)}
            >
              Cancel
            </button>
          </div>
        </div>
      )}
      {error && (
        <p className="error small" role="alert">
          {error.message}
        </p>
      )}
      <input
        className="result-url"
        readOnly
        value={path.url}
        aria-label={`URL of /${path.path}`}
        onFocus={(e) => e.target.select()}
      />
      <CopyButtons url={path.url} />
    </li>
  );
}
