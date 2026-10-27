import type { ReactNode } from "react";
import { Navigate, useLocation } from "react-router";
import { useSession } from "../session";

/** Renders its children only for a logged-in Owner; otherwise goes to login. */
export function RequireOwner({ children }: { children: ReactNode }) {
  const session = useSession();
  const location = useLocation();

  if (session.isPending) {
    return <p className="centered muted">Loading…</p>;
  }
  if (session.isError) {
    return (
      <p className="centered error">Could not reach the server: {session.error.message}</p>
    );
  }
  if (session.data === null) {
    const from = location.pathname + location.search;
    return <Navigate to="/login" replace state={{ from }} />;
  }
  return children;
}
