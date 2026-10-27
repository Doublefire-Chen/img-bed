import { type FormEvent, useState } from "react";
import { Navigate, useLocation, useNavigate } from "react-router";
import { ApiError } from "../api";
import { useLogin, useSession } from "../session";

function describe(error: unknown): string {
  if (error instanceof ApiError) {
    switch (error.code) {
      case "wrong_password":
        return "Wrong password.";
      case "no_password":
        return "No password has been set yet. Run `img-bed set-password` on the server.";
      case "too_many_attempts":
        return `Too many failed attempts. ${error.message.replace(/^too many failed [^;]*; /, "")}`;
    }
    return error.message;
  }
  return "Could not reach the server.";
}

export function LoginPage() {
  const session = useSession();
  const login = useLogin();
  const navigate = useNavigate();
  const location = useLocation();
  const [password, setPassword] = useState("");
  const [rememberMe, setRememberMe] = useState(false);

  const from = (location.state as { from?: string } | null)?.from ?? "/";

  if (session.data) {
    return <Navigate to={from} replace />;
  }

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    login.mutate(
      { password, rememberMe },
      {
        onSuccess: () => navigate(from, { replace: true }),
        onError: () => setPassword(""),
      },
    );
  };

  return (
    <main className="login">
      <form className="card login-card" onSubmit={onSubmit}>
        <h1 className="brand">img-bed</h1>
        <label className="field">
          <span>Password</span>
          <input
            type="password"
            autoComplete="current-password"
            autoFocus
            required
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={rememberMe}
            onChange={(e) => setRememberMe(e.target.checked)}
          />
          <span>Remember me for 7 days</span>
        </label>
        {login.isError && (
          <p className="error" role="alert">
            {describe(login.error)}
          </p>
        )}
        <button type="submit" className="button-primary" disabled={login.isPending}>
          {login.isPending ? "Logging in…" : "Log in"}
        </button>
      </form>
    </main>
  );
}
