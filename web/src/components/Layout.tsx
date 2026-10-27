import { NavLink, Outlet } from "react-router";
import { useLogout } from "../session";

// Sections arrive in later slices; until then they are shown but disabled.
const upcoming = ["API Tokens"];

export function Layout() {
  const logout = useLogout();

  return (
    <div className="shell">
      <header className="topbar">
        <NavLink to="/" className="brand">
          img-bed
        </NavLink>
        <nav className="nav" aria-label="Main">
          <NavLink to="/" end>
            Upload
          </NavLink>
          <NavLink to="/library">Library</NavLink>
          <NavLink to="/trash">Trash</NavLink>
          {upcoming.map((label) => (
            <span key={label} className="nav-disabled" title="Coming soon" aria-disabled="true">
              {label}
            </span>
          ))}
          <NavLink to="/settings">Settings</NavLink>
        </nav>
        <button
          type="button"
          className="button-quiet"
          onClick={() => logout.mutate()}
          disabled={logout.isPending}
        >
          Log out
        </button>
      </header>
      <main className="content">
        <Outlet />
      </main>
    </div>
  );
}
