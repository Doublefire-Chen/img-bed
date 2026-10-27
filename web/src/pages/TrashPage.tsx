import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useSearchParams } from "react-router";
import { api, type Image, imageFileSrc, LIBRARY_KEY, TRASH_KEY } from "../api";
import { DeleteConfirm } from "../components/DeleteConfirm";
import { formatDate, formatDateTime, formatSize } from "../format";

const PER_PAGE = 60;

/**
 * The Trash: deleted Images, most recently deleted first, each with the date
 * it will be Permanently Deleted, a Restore button and "Delete permanently".
 */
export function TrashPage() {
  const [params, setParams] = useSearchParams();
  const page = Math.max(1, Number.parseInt(params.get("page") ?? "1", 10) || 1);
  const query = useQuery({
    queryKey: [...TRASH_KEY, { page }],
    queryFn: () => api.trashPage(page, PER_PAGE),
    placeholderData: keepPreviousData,
  });
  const data = query.data;
  const goTo = (next: number) => {
    setParams(next > 1 ? { page: String(next) } : {});
    window.scrollTo({ top: 0 });
  };

  return (
    <section>
      <header className="library-header">
        <h1>Trash</h1>
        {data && (
          <p className="muted library-stats">
            {data.total} {data.total === 1 ? "Image" : "Images"} · each is Permanently Deleted{" "}
            {data.trash_days} days after it was deleted
          </p>
        )}
      </header>

      {query.isError && (
        <p className="error" role="alert">
          Could not load the Trash: {query.error.message}
        </p>
      )}
      {query.isPending && <p className="muted">Loading…</p>}

      {data && data.images.length === 0 && (
        <div className="empty">
          {data.total === 0 ? (
            <p>The Trash is empty.</p>
          ) : (
            <p>
              There is no page {page}.{" "}
              <button type="button" className="button-small" onClick={() => goTo(1)}>
                Go to the first page
              </button>
            </p>
          )}
        </div>
      )}

      {data && data.images.length > 0 && (
        <>
          <ul className={`trash-list${query.isPlaceholderData ? " grid-stale" : ""}`}>
            {data.images.map((image) => (
              <TrashItem key={image.id} image={image} />
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

function TrashItem({ image }: { image: Image }) {
  const [confirming, setConfirming] = useState(false);
  const queryClient = useQueryClient();
  // Everything Trash-related sits under the Library key.
  const refresh = () => queryClient.invalidateQueries({ queryKey: LIBRARY_KEY });
  const restore = useMutation({ mutationFn: () => api.restore([image.id]), onSuccess: refresh });
  const remove = useMutation({
    mutationFn: () => api.deletePermanently([image.id]),
    onSuccess: refresh,
  });
  const busy = restore.isPending || remove.isPending;
  const paths = image.paths.map((p) => `/${p.path}`);

  return (
    <li className="trash-item">
      <img className="thumb" src={imageFileSrc(image.id)} alt="" loading="lazy" />
      <div className="trash-body">
        <p>
          <Link to={`/library/${image.id}`}>{image.original_filename || `Image ${image.id}`}</Link>{" "}
          <span className="muted small">{formatSize(image.size)}</span>
        </p>
        <p className="small trash-paths">
          <span className="muted">{paths.length === 1 ? "Path" : `${paths.length} Paths`}:</span>{" "}
          <code>{paths.join("  ")}</code>
        </p>
        <p className="muted small">
          Deleted <time title={formatDateTime(image.trashed_at!)}>{formatDate(image.trashed_at!)}</time>{" "}
          · Permanently Deleted on{" "}
          <time title={formatDateTime(image.permanent_delete_at!)}>
            <strong>{formatDate(image.permanent_delete_at!)}</strong>
          </time>
        </p>
        {!confirming && (
          <div className="copy-buttons">
            <button
              type="button"
              className="button-small button-primary"
              disabled={busy}
              onClick={() => {
                remove.reset();
                restore.mutate();
              }}
            >
              {restore.isPending ? "Restoring…" : "Restore"}
            </button>
            <button
              type="button"
              className="button-small button-danger"
              disabled={busy}
              onClick={() => {
                restore.reset();
                setConfirming(true);
              }}
            >
              Delete permanently
            </button>
          </div>
        )}
        {restore.isError && (
          <p className="error small" role="alert">
            {restore.error.message}
          </p>
        )}
        {confirming && (
          <DeleteConfirm
            images={[image]}
            permanent
            pending={remove.isPending}
            error={remove.error}
            onConfirm={() => remove.mutate()}
            onCancel={() => setConfirming(false)}
          />
        )}
      </div>
    </li>
  );
}
