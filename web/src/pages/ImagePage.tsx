import { useQuery } from "@tanstack/react-query";
import { Link, useLocation, useParams } from "react-router";
import { ApiError, api, type Image, LIBRARY_KEY } from "../api";
import { CopyButtons } from "../components/CopyButtons";
import { formatDateTime, formatSize, previewSrc } from "../format";

/** Everything about one Image: preview, facts and all its Paths. */
export function ImagePage() {
  const { id } = useParams();
  const location = useLocation();
  // The Library page and search we came from, if any.
  const from = (location.state as { librarySearch?: string } | null)?.librarySearch ?? "";
  const imageId = Number(id);
  const valid = Number.isSafeInteger(imageId) && imageId > 0;

  const query = useQuery({
    queryKey: [...LIBRARY_KEY, "image", imageId],
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

        <section className="detail-paths" aria-labelledby="paths-heading">
          <div className="section-header">
            <h2 id="paths-heading">
              Paths <span className="muted small">({image.paths.length})</span>
            </h2>
            {/* Adding a Path arrives with Path management. */}
          </div>
          <ul className="path-links">
            {image.paths.map((p) => (
              <li key={p.path} className={p.is_primary ? "path-primary" : undefined}>
                <p className="result-path">
                  <code>/{p.path}</code>
                  {p.is_primary && <span className="badge small">Primary</span>}
                  {/* Per-Path actions (set Primary, remove) go here. */}
                </p>
                <input
                  className="result-url"
                  readOnly
                  value={p.url}
                  aria-label={`URL of /${p.path}`}
                  onFocus={(e) => e.target.select()}
                />
                <CopyButtons url={p.url} />
              </li>
            ))}
          </ul>
        </section>
      </div>
    </div>
  );
}
