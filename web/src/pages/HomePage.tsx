import { useSession } from "../session";

export function HomePage() {
  const { data: session } = useSession();
  const expires = session ? new Date(session.expires_at * 1000) : null;

  return (
    <section>
      <h1>Library</h1>
      <div className="empty">
        <p>No Images yet.</p>
        <p className="muted">Uploading and browsing Images are coming soon.</p>
      </div>
      {expires && (
        <p className="muted small">
          Logged in until {expires.toLocaleString()}.
        </p>
      )}
    </section>
  );
}
