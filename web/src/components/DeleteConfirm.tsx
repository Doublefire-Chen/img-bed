import type { Image } from "../api";

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** How many Images and Paths a delete affects, e.g. "2 Images" and "5 Paths". */
export function deleteCounts(images: Image[]): { images: string; paths: string } {
  const paths = images.reduce((sum, image) => sum + image.paths.length, 0);
  return { images: plural(images.length, "Image", "Images"), paths: plural(paths, "Path", "Paths") };
}

/**
 * The in-page confirmation before Images go to the Trash (`permanent` false)
 * or are Permanently Deleted: states how many Images and Paths stop working.
 */
export function DeleteConfirm({
  images,
  permanent = false,
  pending,
  error,
  onConfirm,
  onCancel,
}: {
  images: Image[];
  permanent?: boolean;
  pending: boolean;
  error: Error | null;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const counts = deleteCounts(images);
  const one = images.length === 1;
  return (
    <div className="confirm" role="alertdialog" aria-label={permanent ? "Delete permanently" : "Delete"}>
      {permanent ? (
        <p className="small">
          Permanently delete {counts.images}? {one ? "Its file is" : "Their files are"} erased and this
          cannot be undone. {counts.paths} become free for any Image.
        </p>
      ) : (
        <p className="small">
          Delete {counts.images}? <strong>{counts.paths} will stop working</strong>.{" "}
          {one ? "It stays" : "They stay"} in the Trash for 30 days and can be Restored with every
          Path until then.
        </p>
      )}
      {error && (
        <p className="error small" role="alert">
          {error.message}
        </p>
      )}
      <div className="copy-buttons">
        <button
          type="button"
          className="button-small button-danger-solid"
          disabled={pending}
          autoFocus
          onClick={onConfirm}
        >
          {pending ? "Deleting…" : permanent ? "Delete permanently" : `Delete ${counts.images}`}
        </button>
        <button type="button" className="button-small button-quiet" disabled={pending} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}
