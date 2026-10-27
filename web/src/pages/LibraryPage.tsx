import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link, useLocation, useSearchParams } from "react-router";
import { api, LIBRARY_KEY } from "../api";
import { formatDate, formatDateTime, formatSize, previewSrc } from "../format";

/** Images per page in the grid. */
const PER_PAGE = 60;
/** How long typing pauses before the search runs. */
const SEARCH_DELAY_MS = 250;

/**
 * The Library: every Image (not in the Trash), newest first, in pages, with a
 * search by Path or original filename. The page and search live in the URL,
 * so coming back from an Image keeps the place.
 */
export function LibraryPage() {
  const [params, setParams] = useSearchParams();
  const location = useLocation();
  const search = params.get("q") ?? "";
  const page = Math.max(1, Number.parseInt(params.get("page") ?? "1", 10) || 1);
  const [typed, setTyped] = useState(search);

  // Follow the URL when it changes from outside (back/forward).
  useEffect(() => setTyped(search), [search]);

  // Run the search once typing pauses; a new search starts at page 1.
  useEffect(() => {
    if (typed === search) return;
    const timer = window.setTimeout(() => {
      setParams(typed.trim() ? { q: typed } : {}, { replace: true });
    }, SEARCH_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [typed, search, setParams]);

  const goTo = (next: number) => {
    const p: Record<string, string> = {};
    if (search) p.q = search;
    if (next > 1) p.page = String(next);
    setParams(p);
    window.scrollTo({ top: 0 });
  };

  const query = useQuery({
    queryKey: [...LIBRARY_KEY, "page", { page, search }],
    queryFn: () => api.libraryPage(page, PER_PAGE, search),
    placeholderData: keepPreviousData,
  });
  const data = query.data;
  const stats = data?.stats;

  return (
    <section>
      <header className="library-header">
        <h1>Library</h1>
        {stats && (
          <p className="muted library-stats" aria-label="Library totals">
            {stats.image_count} {stats.image_count === 1 ? "Image" : "Images"} ·{" "}
            <span title={`${stats.disk_usage.toLocaleString()} bytes on disk (Trash included)`}>
              {formatSize(stats.disk_usage)}
            </span>
          </p>
        )}
        <input
          className="search"
          type="search"
          placeholder="Search by Path or filename"
          aria-label="Search by Path or original filename"
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
        />
      </header>

      {query.isError && (
        <p className="error" role="alert">
          Could not load the Library: {query.error.message}
        </p>
      )}
      {query.isPending && <p className="muted">Loading…</p>}

      {data && data.images.length === 0 && (
        <div className="empty">
          {search ? (
            <p>No Image has a Path or filename containing “{search}”.</p>
          ) : data.total === 0 ? (
            <>
              <p>No Images yet.</p>
              <p className="muted">
                <Link to="/">Upload</Link> some to see them here.
              </p>
            </>
          ) : (
            <p>
              There is no page {page}. <button type="button" className="button-small" onClick={() => goTo(1)}>Go to the first page</button>
            </p>
          )}
        </div>
      )}

      {data && data.images.length > 0 && (
        <>
          {search && (
            <p className="muted small">
              {data.total} {data.total === 1 ? "match" : "matches"} for “{search}”
            </p>
          )}
          <ul className={`grid${query.isPlaceholderData ? " grid-stale" : ""}`}>
            {data.images.map((image) => (
              <li key={image.id}>
                <Link
                  to={`/library/${image.id}`}
                  state={{ librarySearch: location.search }}
                  className="tile" title={image.original_filename}>
                  <img className="tile-image" src={previewSrc(image.url)} alt="" loading="lazy" />
                  <span className="tile-path">/{image.path}</span>
                  <span className="tile-meta muted small">
                    <time title={formatDateTime(image.uploaded_at)}>{formatDate(image.uploaded_at)}</time>
                    {" · "}
                    {formatSize(image.size)}
                  </span>
                </Link>
              </li>
            ))}
          </ul>
          {data.pages > 1 && (
            <nav className="pager" aria-label="Pages">
              <button type="button" onClick={() => goTo(page - 1)} disabled={page <= 1}>
                Newer
              </button>
              <span className="muted small">
                Page {page} of {data.pages}
              </span>
              <button type="button" onClick={() => goTo(page + 1)} disabled={page >= data.pages}>
                Older
              </button>
            </nav>
          )}
        </>
      )}
    </section>
  );
}
