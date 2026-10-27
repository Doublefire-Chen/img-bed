import { Link } from "react-router";
import { useSession } from "../session";

export function LibraryPage() {
  const { data: session } = useSession();
  const expires = session ? new Date(session.expires_at * 1000) : null;

  return (
    <section>
      <h1>Library</h1>
      <div className="empty">
        <p>Browsing Images is coming soon.</p>
        <p className="muted">
          Until then, links are shown right after each upload on the <Link to="/">Upload</Link> page.
        </p>
      </div>
      {expires && (
        <p className="muted small">
          Logged in until {expires.toLocaleString()}.
        </p>
      )}
    </section>
  );
}
